use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use kani_core::error::{Error, Result};
use kani_core::wasm::kani::extension::types::{
    Chapter, ChapterInfo, ChapterList, FilterDef, FilterList, FilterOption, FilterSemantic,
    FilterState, FilterTypeTag, MangaInfo, MangaList, MangaListItem, MangaStatus, OptionState,
    Page, PrefKind, PreferenceSpec, SortOption,
};
use kani_core::wasm::AllowedHost;

pub struct YamlSource {
    pub config: Arc<kani_yaml::ValidatedExtension>,
    http: kani_core::http::SmartClient,
    cache: Arc<dyn kani_core::cache::CacheBackend>,
    cache_namespace: String,
    v8_process: kani_core::v8_process::V8ProcessHandle,
    prefs: Arc<std::sync::RwLock<HashMap<String, String>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    hook_registry: Option<Arc<kani_core::scripting::HookRegistry>>,
    pure_fn_registry: Option<Arc<kani_core::scripting::PureFunctionRegistry>>,
    max_hook_requests: u32,
}

impl YamlSource {
    pub fn new(
        config: Arc<kani_yaml::ValidatedExtension>,
        http: kani_core::http::SmartClient,
        cache: Arc<dyn kani_core::cache::CacheBackend>,
        cache_namespace: String,
        preferences: HashMap<String, String>,
    ) -> Self {
        let max_concurrent = config
            .metadata
            .rate_limit
            .as_ref()
            .map(|rl| rl.max_concurrent as usize)
            .unwrap_or(4);
        let max_hook_requests = config
            .metadata
            .rate_limit
            .as_ref()
            .map(|rl| rl.max_hook_requests)
            .unwrap_or(3);

        let hook_scripts = kani_core::scripting::HookScripts {
            pre_request: config.pre_request.clone(),
            on_status: config.on_status.clone(),
            endpoint_pre_request: config.endpoint_pre_request.clone(),
            endpoint_on_status: config.endpoint_on_status.clone(),
        };
        let hook_registry = if hook_scripts.pre_request.is_some()
            || !hook_scripts.on_status.is_empty()
            || !hook_scripts.endpoint_pre_request.is_empty()
            || !hook_scripts.endpoint_on_status.is_empty()
        {
            kani_core::scripting::HookRegistry::compile(&hook_scripts)
                .ok()
                .map(Arc::new)
        } else {
            None
        };

        let pure_fn_registry = if !config.pure_scripts.is_empty() {
            kani_core::scripting::PureFunctionRegistry::compile(&config.pure_scripts)
                .ok()
                .map(Arc::new)
        } else {
            None
        };

        Self {
            config,
            http,
            cache,
            cache_namespace,
            v8_process: Arc::new(Mutex::new(None)),
            prefs: Arc::new(std::sync::RwLock::new(preferences)),
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            hook_registry,
            pure_fn_registry,
            max_hook_requests,
        }
    }

    pub fn update_preferences(&self, prefs: HashMap<String, String>) {
        if let Ok(mut lock) = self.prefs.write() {
            *lock = prefs;
        }
    }

    fn make_host_state(&self) -> Result<kani_core::wasm::HostState> {
        let allowed_host = if self.config.unrestricted_http {
            AllowedHost::Unrestricted
        } else {
            AllowedHost::Restricted(self.config.base_url.clone())
        };
        let mut state = kani_core::wasm::HostState::new(
            self.http.clone(),
            allowed_host,
            Arc::clone(&self.cache),
            self.cache_namespace.clone(),
            Arc::clone(&self.v8_process),
        )?;
        state.preferences = self
            .prefs
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        state.hook_registry = self.hook_registry.clone();
        state.pure_fn_registry = self.pure_fn_registry.clone();
        state.max_hook_requests = self.max_hook_requests;
        Ok(state)
    }

    async fn acquire(&self) -> Result<tokio::sync::OwnedSemaphorePermit> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| Error::Internal("Source concurrency semaphore closed".into()))
    }

    fn build_args(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn make_request(
        ep: &kani_yaml::ValidatedEndpoint,
        ext: &kani_yaml::ValidatedExtension,
        args: &HashMap<String, String>,
        endpoint_name: &str,
    ) -> kani_shared::ast::RequestDef {
        let mut resolved = args.clone();
        kani_yaml::resolve_composite_ids(ep, &mut resolved);
        let url = kani_yaml::build_url_with_args(&ext.base_url, &ep.route, &resolved);
        let queries = kani_yaml::build_queries(&ep.queries, &resolved);
        kani_shared::ast::RequestDef {
            url,
            method: ep.method.clone(),
            headers: ep.headers.clone(),
            queries,
            endpoint_id: Some(endpoint_name.to_string()),
        }
    }

    async fn eval_endpoint_once(
        &self,
        ep: &kani_yaml::ValidatedEndpoint,
        endpoint_name: &str,
        args: &HashMap<String, String>,
    ) -> std::result::Result<serde_json::Value, String> {
        use kani_core::evaluator::{html_eval, json_eval};
        use kani_yaml::yaml::schema::ResponseType;

        let req = Self::make_request(ep, &self.config, args, endpoint_name);
        let bp = kani_yaml::build_blueprint(ep, &self.config, endpoint_name, req);
        let mut state = self
            .make_host_state()
            .map_err(|e| e.to_string())?;

        match ep.response_type {
            ResponseType::Html => html_eval::extract_html(&mut state, None, &bp).await,
            ResponseType::Json => json_eval::extract_json(&mut state, None, &bp).await,
        }
    }

    async fn eval_endpoint(
        &self,
        ep: &kani_yaml::ValidatedEndpoint,
        endpoint_name: &str,
        args: &HashMap<String, String>,
    ) -> Result<serde_json::Value> {
        use kani_yaml::yaml::schema::EndpointVia;

        if ep.via == Some(EndpointVia::BrowserPayload) {
            return Err(Error::Internal(
                "browser_payload endpoint requires browser runtime (not yet enabled)".into(),
            ));
        }

        let mut retries_left = self.max_hook_requests;
        loop {
            match self.eval_endpoint_once(ep, endpoint_name, args).await {
                Ok(v) => return Ok(v),
                Err(ref e) if e.starts_with("__refresh_auth__:") => {
                    if retries_left == 0 {
                        return Err(Error::Extension(
                            kani_shared::extension::ExtensionError::parse(
                                "RefreshAuth: max retries exceeded".to_string(),
                            ),
                        ));
                    }
                    retries_left -= 1;
                    let auth_endpoint_name =
                        e.strip_prefix("__refresh_auth__:").unwrap_or("login");
                    if let Some(auth_ep) = self.config.endpoint_by_name(auth_endpoint_name) {
                        let auth_args = HashMap::new();
                        let _ = self
                            .eval_endpoint_once(auth_ep, auth_endpoint_name, &auth_args)
                            .await;
                    }
                }
                Err(e) => {
                    return Err(Error::Extension(
                        kani_shared::extension::ExtensionError::parse(e),
                    ))
                }
            }
        }
    }

    pub async fn get_metadata(&self) -> Result<String> {
        let rl = self.config.metadata.rate_limit.as_ref().map(|r| {
            kani_shared::extension::RateLimitConfig {
                requests_per_second: r.requests_per_second as f32,
                burst: r.burst,
                max_concurrent: r.max_concurrent,
                max_hook_requests: r.max_hook_requests,
            }
        });
        let meta = kani_shared::extension::ExtensionMetadata {
            id: self.config.id.clone(),
            name: self.config.name.clone(),
            version: self.config.version.clone(),
            base_url: self.config.base_url.clone(),
            language: self.config.language.clone(),
            nsfw: self.config.nsfw,
            unrestricted_http: self.config.unrestricted_http,
            mihon_source_id: self.config.mihon_source_id,
            rate_limit: rl,
            icon: self.config.metadata.icon.clone(),
            languages: self.config.metadata.languages.clone(),
            description: self.config.metadata.description.clone(),
            schema_version: self.config.schema_version,
            min_kani_version: self.config.min_kani_version.clone(),
            requires_capabilities: self.config.requires_capabilities.clone(),
            sections: self
                .config
                .metadata
                .sections
                .iter()
                .map(|s| kani_shared::extension::Section {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    nsfw: s.nsfw,
                })
                .collect(),
            scripts: std::collections::BTreeMap::new(),
            pre_request: self.config.pre_request.clone(),
            on_status: self.config.on_status.clone(),
            endpoint_pre_request: self.config.endpoint_pre_request.clone(),
            endpoint_on_status: self.config.endpoint_on_status.clone(),
        };
        serde_json::to_string(&meta).map_err(Error::Json)
    }

    pub async fn get_popular_manga(
        &self,
        page: i32,
        page_size: i32,
        filters: &[kani_shared::types::ActiveFilter],
    ) -> Result<MangaList> {
        use kani_yaml::yaml::model::ValidatedPopular;

        let _permit = self.acquire().await?;

        match &self.config.popular {
            Some(ValidatedPopular::Delegated {
                delegate_to: _,
                empty_without_filters,
            }) => {
                if *empty_without_filters && filters.is_empty() {
                    return Ok(MangaList {
                        manga: vec![],
                        has_next_page: false,
                        total_pages: None,
                    });
                }
                self.search_inner("", page, page_size).await
            }
            Some(ValidatedPopular::Full(ep)) => {
                let args = Self::build_args(&[
                    ("page", &page.to_string()),
                    ("page_size", &page_size.to_string()),
                ]);
                let result = self.eval_endpoint(ep, "popular", &args).await?;
                Ok(unpack_manga_list(&result, ep))
            }
            None => Err(Error::Extension(
                kani_shared::extension::ExtensionError::parse(
                    "popular endpoint not configured".to_string(),
                ),
            )),
        }
    }

    async fn search_inner(&self, query: &str, page: i32, page_size: i32) -> Result<MangaList> {
        let ep = self.config.search.as_ref().ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "search endpoint not configured".to_string(),
            ))
        })?;
        let page_str = page.to_string();
        let page_size_str = page_size.to_string();
        let args = Self::build_args(&[
            ("query", query),
            ("page", &page_str),
            ("page_size", &page_size_str),
        ]);
        let result = self.eval_endpoint(ep, "search", &args).await?;
        Ok(unpack_manga_list(&result, ep))
    }

    pub async fn search_manga(
        &self,
        query: &str,
        page: i32,
        page_size: i32,
        _filters: &[kani_shared::types::ActiveFilter],
    ) -> Result<MangaList> {
        let _permit = self.acquire().await?;
        self.search_inner(query, page, page_size).await
    }

    pub async fn get_manga_details(&self, manga_id: &str) -> Result<MangaInfo> {
        let _permit = self.acquire().await?;

        let ep = self.config.manga_details.as_ref().ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "manga_details endpoint not configured".to_string(),
            ))
        })?;

        let args = Self::build_args(&[("manga_id", manga_id)]);
        let result = self.eval_endpoint(ep, "manga_details", &args).await?;
        unpack_manga_info(&result)
    }

    pub async fn get_chapter_list(
        &self,
        manga_id: &str,
        page: i32,
        page_size: Option<i32>,
        sort: Option<String>,
    ) -> Result<ChapterList> {
        let _permit = self.acquire().await?;

        let ep = self.config.chapter_list.as_ref().ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "chapter_list endpoint not configured".to_string(),
            ))
        })?;

        let page_str = page.to_string();
        let page_size_str = page_size.unwrap_or(100).to_string();
        let sort_str = sort.as_deref().unwrap_or("");
        let args = Self::build_args(&[
            ("manga_id", manga_id),
            ("page", &page_str),
            ("page_size", &page_size_str),
            ("sort", sort_str),
        ]);
        let result = self.eval_endpoint(ep, "chapter_list", &args).await?;
        Ok(unpack_chapter_list(&result, ep))
    }

    pub async fn get_pages(&self, manga_id: &str, chapter_id: &str) -> Result<Chapter> {
        let _permit = self.acquire().await?;

        let ep = self.config.pages.as_ref().ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "pages endpoint not configured".to_string(),
            ))
        })?;

        let args = Self::build_args(&[("manga_id", manga_id), ("chapter_id", chapter_id)]);
        let result = self.eval_endpoint(ep, "pages", &args).await?;
        Ok(unpack_chapter(&result))
    }

    pub async fn get_chapter_sort_list(&self) -> Result<Vec<SortOption>> {
        let opts = self
            .config
            .chapter_sort
            .as_ref()
            .map(|cs| {
                cs.options
                    .iter()
                    .map(|o| SortOption {
                        id: o.id.clone(),
                        name: o.label.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(opts)
    }

    pub async fn get_source_url(&self, manga_id: &str) -> Result<String> {
        let template = self.config.get_url.as_deref().ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "get_url not configured".to_string(),
            ))
        })?;
        let args = Self::build_args(&[("manga_id", manga_id)]);
        Ok(kani_yaml::build_url_with_args(
            &self.config.base_url,
            template,
            &args,
        ))
    }

    pub async fn get_filter_list(&self) -> Result<FilterList> {
        use kani_yaml::yaml::schema::{FilterDefault, FilterKind, FilterSemantic as YSemantic, OptionSetDef};

        let filters = self
            .config
            .filters
            .iter()
            .map(|entry| {
                let tag = match entry.kind {
                    FilterKind::Select => FilterTypeTag::Select,
                    FilterKind::Checkbox => FilterTypeTag::Checkbox,
                    FilterKind::TextInput => FilterTypeTag::TextInput,
                    FilterKind::Sort => FilterTypeTag::Sort,
                    FilterKind::Multiselect => FilterTypeTag::Multiselect,
                    FilterKind::IntRange | FilterKind::DateRange => FilterTypeTag::TextInput,
                };

                let options: Vec<FilterOption> = if let Some(ref key) = entry.options_ref {
                    match self.config.option_sets.get(key) {
                        Some(OptionSetDef::Static(items)) => items
                            .iter()
                            .map(|item| FilterOption {
                                filter_name: entry.id.clone(),
                                name: item.name.clone(),
                                value: item.value.clone(),
                            })
                            .collect(),
                        _ => vec![],
                    }
                } else {
                    entry
                        .options
                        .iter()
                        .map(|o| FilterOption {
                            filter_name: entry.id.clone(),
                            name: o.name.clone(),
                            value: o.value.clone(),
                        })
                        .collect()
                };

                let default_value = entry.default.as_ref().map(|d| match d {
                    FilterDefault::Bool(b) => FilterState::Checkbox(*b),
                    FilterDefault::Option { name, value } => {
                        FilterState::Selection(OptionState {
                            name: name.clone(),
                            value: value.clone(),
                        })
                    }
                    FilterDefault::Text(t) => FilterState::TextInput(t.clone()),
                });

                let semantic = entry.semantic.as_ref().map(|s| match s {
                    YSemantic::Author => FilterSemantic::Author,
                    YSemantic::Artist => FilterSemantic::Artist,
                    YSemantic::Tag => FilterSemantic::Tag,
                });

                FilterDef {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    tag,
                    options,
                    default_value,
                    semantic,
                }
            })
            .collect();

        Ok(FilterList { filters })
    }

    pub async fn get_preferences(&self) -> Result<Vec<PreferenceSpec>> {
        use kani_yaml::yaml::schema::PreferenceKind;

        let specs = self
            .config
            .preferences
            .iter()
            .map(|entry| {
                let kind = match entry.kind {
                    PreferenceKind::Toggle => PrefKind::Toggle,
                    PreferenceKind::Select => PrefKind::Select,
                    PreferenceKind::Text => PrefKind::Text,
                    PreferenceKind::MultiValueList => PrefKind::MultiValueList,
                };
                PreferenceSpec {
                    key: entry.key.clone(),
                    label: entry.label.clone(),
                    kind,
                    options: entry
                        .options
                        .iter()
                        .map(|o| (o.name.clone(), o.value.clone()))
                        .collect(),
                    default: entry.default.clone(),
                    description: entry.description.clone(),
                    secret: entry.secret,
                }
            })
            .collect();

        Ok(specs)
    }

    pub async fn fetch_page_list(
        &self,
        manga_id: &str,
        chapter_id: &str,
    ) -> Result<(Chapter, String)> {
        let chapter = self.get_pages(manga_id, chapter_id).await?;
        Ok((chapter, self.config.base_url.clone()))
    }
}

fn unpack_manga_list(
    result: &serde_json::Value,
    ep: &kani_yaml::ValidatedEndpoint,
) -> MangaList {
    use kani_yaml::yaml::model::{ValidatedHnp, ValidatedTotalPages};

    let empty = vec![];
    let rows = result["rows"].as_array().unwrap_or(&empty);
    let has_next_page = match &ep.has_next_page {
        ValidatedHnp::Static(b) => *b,
        _ => result["scalars"]["has_next_page"]
            .as_bool()
            .unwrap_or(false),
    };
    let total_pages = match &ep.total_pages {
        ValidatedTotalPages::Static(n) => Some(*n),
        ValidatedTotalPages::None => None,
        ValidatedTotalPages::Scalar(_) => {
            result["scalars"]["total_pages"].as_u64().map(|n| n as u32)
        }
    };
    let manga = rows
        .iter()
        .filter_map(|row| {
            let id = row["id"].as_str()?.to_string();
            let title = row["title"].as_str()?.to_string();
            let cover_url = row["cover_url"].as_str().map(|s| s.to_string());
            Some(MangaListItem {
                id,
                title,
                cover_url,
            })
        })
        .collect();
    MangaList {
        manga,
        has_next_page,
        total_pages,
    }
}

fn unpack_manga_info(result: &serde_json::Value) -> Result<MangaInfo> {
    let empty = vec![];
    let rows = result["rows"].as_array().unwrap_or(&empty);
    let row = rows.first().ok_or_else(|| {
        Error::Extension(kani_shared::extension::ExtensionError::parse(
            "manga_details: no result row".to_string(),
        ))
    })?;

    let id = row["id"]
        .as_str()
        .ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "manga_details: missing id".to_string(),
            ))
        })?
        .to_string();
    let title = row["title"]
        .as_str()
        .ok_or_else(|| {
            Error::Extension(kani_shared::extension::ExtensionError::parse(
                "manga_details: missing title".to_string(),
            ))
        })?
        .to_string();
    let status = match row["status"].as_str() {
        Some("ongoing") => MangaStatus::Ongoing,
        Some("completed") => MangaStatus::Completed,
        Some("hiatus") => MangaStatus::Hiatus,
        Some("cancelled") => MangaStatus::Cancelled,
        _ => MangaStatus::Unknown,
    };
    Ok(MangaInfo {
        id,
        title,
        cover_url: row["cover_url"].as_str().map(|s| s.to_string()),
        description: row["description"].as_str().map(|s| s.to_string()),
        authors: str_array(row, "authors"),
        artists: str_array(row, "artists"),
        status,
        tags: str_array(row, "tags"),
    })
}

fn str_array(row: &serde_json::Value, key: &str) -> Vec<String> {
    row[key]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn unpack_chapter_list(
    result: &serde_json::Value,
    ep: &kani_yaml::ValidatedEndpoint,
) -> ChapterList {
    use kani_yaml::yaml::model::{ValidatedHnp, ValidatedTotalPages};

    let empty = vec![];
    let rows = result["rows"].as_array().unwrap_or(&empty);
    let has_next_page = match &ep.has_next_page {
        ValidatedHnp::Static(b) => *b,
        _ => result["scalars"]["has_next_page"]
            .as_bool()
            .unwrap_or(false),
    };
    let total_pages = match &ep.total_pages {
        ValidatedTotalPages::Static(n) => Some(*n),
        ValidatedTotalPages::None => None,
        ValidatedTotalPages::Scalar(_) => {
            result["scalars"]["total_pages"].as_u64().map(|n| n as u32)
        }
    };
    let chapters = rows
        .iter()
        .filter_map(|row| {
            let id = row["id"].as_str()?.to_string();
            Some(ChapterInfo {
                id,
                number: row["number"].as_f64().unwrap_or(0.0),
                title: row["title"].as_str().map(|s| s.to_string()),
                volume: row["volume"].as_i64().map(|v| v as i32),
                scanlator: row["scanlator"].as_str().map(|s| s.to_string()),
                date_uploaded: row["date_uploaded"].as_i64(),
                language: row["language"].as_str().unwrap_or("en").to_string(),
            })
        })
        .collect();
    ChapterList {
        chapters,
        has_next_page,
        total_pages,
    }
}

fn unpack_chapter(result: &serde_json::Value) -> Chapter {
    let empty = vec![];
    let rows = result["rows"].as_array().unwrap_or(&empty);
    let pages = rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let url = row["url"].as_str()?.to_string();
            let index = row["index"].as_i64().unwrap_or(i as i64) as i32;
            Some(Page {
                index,
                url,
                transform: None,
            })
        })
        .collect();
    Chapter { pages }
}

#[cfg(test)]
impl YamlSource {
    pub(crate) fn for_test() -> Self {
        let cache = Arc::new(kani_core::cache::InMemoryCache::new());
        Self {
            config: Arc::new(kani_yaml::ValidatedExtension::default()),
            http: kani_core::http::SmartClient::new(None)
                .expect("SmartClient::new"),
            cache,
            cache_namespace: String::new(),
            v8_process: Arc::new(Mutex::new(None)),
            prefs: Arc::new(std::sync::RwLock::new(HashMap::new())),
            semaphore: Arc::new(tokio::sync::Semaphore::new(4)),
            hook_registry: None,
            pure_fn_registry: None,
            max_hook_requests: 3,
        }
    }
}
