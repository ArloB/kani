use super::*;

impl AppService {
    pub async fn get_source(&self, id: i64) -> Result<Source> {
        let source = sqlx::query_as!(Source, "SELECT * FROM sources WHERE id = ?", id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound("Source not found".into()))?;

        Ok(source)
    }

    pub async fn list_sources(&self) -> Result<Vec<Source>> {
        sqlx::query_as!(Source, "SELECT * FROM sources LIMIT 1000")
            .fetch_all(&self.db)
            .await
            .map_err(Into::into)
    }

    /// Inserts a new source row with a default version and returns its id.
    pub async fn add_source(&self, name: &str, user_id: i64) -> Result<i64> {
        let id = sqlx::query_scalar!(
            "INSERT INTO sources (name, version) VALUES (?, '0.1') RETURNING id",
            name
        )
        .fetch_one(&self.db)
        .await?;

        self.audit(Some(user_id), "source.install", Some(name), None)
            .await;
        Ok(id)
    }

    /// Updates the name and/or version of an existing source.
    pub async fn update_source(
        &self,
        id: i64,
        name: Option<String>,
        version: Option<String>,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE sources SET name = COALESCE(?, name), version = COALESCE(?, version) WHERE id = ?",
            name,
            version,
            id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// Deletes a source, removes its WASM file, evicts it from the runtime, and
    /// invalidates the cache. A no-op if the source does not exist.
    pub async fn delete_source(&self, id: i64, user_id: i64) -> Result<()> {
        let row = sqlx::query!("DELETE FROM sources WHERE id = ? RETURNING name", id)
            .fetch_optional(&self.db)
            .await?;

        if let Some(row) = row {
            let wasm_path = self.settings.read().await.wasm_storage_path.clone();
            let storage = wasm_path
                .to_str()
                .ok_or_else(|| {
                    ServiceError::Internal("wasm_storage_path is not valid UTF-8".into())
                })?
                .to_owned();
            kani_core::file_storage::delete_wasm_file(&storage, &row.name)
                .await
                .map_err(ServiceError::Core)?;

            self.sources.write().await.remove(&id);
            self.cache.invalidate_source(id);
            self.audit(Some(user_id), "source.delete", Some(&row.name), None)
                .await;
        }

        Ok(())
    }

    pub async fn toggle_source_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        sqlx::query!("UPDATE sources SET enabled = ? WHERE id = ?", enabled, id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn toggle_source_favourite(&self, id: i64, favourited: bool) -> Result<()> {
        sqlx::query!(
            "UPDATE sources SET favourited = ? WHERE id = ?",
            favourited,
            id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// Returns the base URL of a source, or an empty string if the source has none.
    pub async fn get_source_base_url(&self, id: i64) -> Result<String> {
        Ok(
            sqlx::query_scalar!("SELECT base_url FROM sources WHERE id = ?", id)
                .fetch_optional(&self.db)
                .await?
                .unwrap_or_default(),
        )
    }

    pub async fn get_metadata(&self, id: i64) -> Result<String> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };

        let result = source_manager
            .lease_instance()
            .await?
            .get_metadata()
            .await?;

        serde_json::to_string(&result).map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
    }

    pub async fn get_popular_manga(
        &self,
        id: i64,
        page: i32,
        page_size: i32,
        filters: Option<String>,
    ) -> Result<String> {
        let filters_key = filters.clone().unwrap_or_default();
        let active_filters: Vec<kani_shared::types::ActiveFilter> = filters
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let sources = self.sources.clone();

        self.cache
            .get_or_fetch_popular_manga(id, page, page_size, filters_key, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_popular_manga(page, page_size, &active_filters)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn search_manga(
        &self,
        id: i64,
        query: &str,
        page: i32,
        page_size: i32,
        filters: Option<String>,
    ) -> Result<String> {
        let filters_key = filters.clone().unwrap_or_default();
        let active_filters: Vec<kani_shared::types::ActiveFilter> = filters
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let sources = self.sources.clone();
        let q = query.to_string();
        self.cache
            .get_or_fetch_search_results(id, query, page, page_size, filters_key, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .search_manga(&q, page, page_size, &active_filters)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_filter_list(&self, id: i64) -> Result<kani_core::WitFilterList> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };
        source_manager
            .lease_instance()
            .await?
            .get_filter_list()
            .await
            .map_err(ServiceError::Core)
    }

    pub async fn get_manga_details(&self, id: i64, manga_id: &str) -> Result<String> {
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);

        self.cache
            .get_or_fetch_manga_details(id, &manga_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_manga_details(&manga_id_d)
                    .await?;
                serde_json::to_string(&convert_to_shared_manga_info(result))
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_pages(&self, id: i64, manga_id: &str, chapter_id: &str) -> Result<String> {
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);
        let chapter_id_d = decode_manga_id(chapter_id);

        self.cache
            .get_or_fetch_pages(id, &manga_id_d.clone(), &chapter_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_pages(&manga_id_d, &chapter_id_d)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_chapter_list_paged(
        &self,
        id: i64,
        manga_id: &str,
        page: i32,
        page_size: i32,
        sort: Option<String>,
    ) -> Result<String> {
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);
        let sort_key = sort.clone().unwrap_or_default();

        self.cache
            .get_or_fetch_chapter_list(id, &manga_id_d.clone(), page, page_size, &sort_key, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_chapter_list(&manga_id_d, page, Some(page_size), sort)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_chapter_sort_list(&self, id: i64) -> Result<Vec<kani_shared::types::ChapterSortOption>> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };
        let wit_opts = source_manager
            .lease_instance()
            .await?
            .get_chapter_sort_list()
            .await?;
        Ok(wit_opts
            .into_iter()
            .map(|o| kani_shared::types::ChapterSortOption { id: o.id, name: o.name })
            .collect())
    }

    pub async fn global_search(
        &self,
        query: &str,
        scope: SearchScope,
        page: i32,
        page_size: i32,
    ) -> Result<Vec<GlobalSearchResult>> {
        let favourited_only = matches!(scope, SearchScope::FavouritedOnly) as i64;

        let ids_to_search: IndexMap<i64, String> = sqlx::query!(
            "SELECT id, name FROM sources WHERE enabled = 1 AND (favourited = 1 OR ? = 0)",
            favourited_only
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .filter(|r| match &scope {
            SearchScope::Sources(ids) => ids.contains(&r.id),
            _ => true,
        })
        .map(|r| (r.id, r.name))
        .collect();

        let tasks: Vec<_> = ids_to_search
            .iter()
            .map(|(&source_id, source_name)| {
                let state = self.clone();
                let q = query.to_string();
                let source_name = source_name.clone();

                tokio::spawn(async move {
                    let result = state.search_manga(source_id, &q, page, page_size, None).await;
                    (source_id, source_name, result)
                })
            })
            .collect();

        let outcomes = futures::future::join_all(tasks).await;

        let mut per_source_results: Vec<GlobalSearchResult> = Vec::new();

        for outcome in outcomes {
            match outcome {
                Ok((source_id, source_name, Ok(json))) => {
                    match serde_json::from_str::<MangaList>(&json) {
                        Ok(manga_list) => {
                            per_source_results.push(GlobalSearchResult {
                                source_id,
                                source_name,
                                has_next_page: manga_list.has_next_page,
                                manga: manga_list.manga,
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse results for source {source_id}: {e}");
                            return Err(ServiceError::Core(kani_core::Error::Json(e)));
                        }
                    }
                }
                Ok((source_id, _, Err(e))) => {
                    tracing::warn!("Search failed for source {source_id}: {e}");
                    per_source_results.push(GlobalSearchResult {
                        source_id,
                        source_name: ids_to_search.get(&source_id).cloned().unwrap_or_default(),
                        has_next_page: false,
                        manga: vec![],
                    });
                }
                Err(join_err) => {
                    tracing::error!("Task panicked: {join_err}");
                }
            }
        }

        Ok(per_source_results)
    }

    pub(super) async fn scan_and_register_sources(
        db: &SqlitePool,
        wasm_storage_path: &std::path::Path,
        smart_client: kani_core::http::SmartClient,
        wasm_runtime: &WasmRuntime,
        preference_schemas: &DashMap<i64, Vec<kani_core::PreferenceSpec>>,
    ) -> Result<()> {
        tracing::info!(
            "Scanning and registering sources in {:?}",
            wasm_storage_path
        );

        let mut entries = tokio::fs::read_dir(wasm_storage_path)
            .await
            .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| ServiceError::Internal("Invalid filename".to_string()))?;

                let exists = sqlx::query!("SELECT id FROM sources WHERE name = ?", filename)
                    .fetch_optional(db)
                    .await?
                    .is_some();

                if !exists {
                    let bytes = tokio::fs::read(&path)
                        .await
                        .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?;

                    let component = wasm_runtime
                        .compile_component(&bytes)
                        .map_err(ServiceError::Core)?;

                    let (metadata, schema) = {
                        let mut inst = kani_core::sources::SourceInstance::new(
                            smart_client.clone(),
                            None,
                            false,
                        );
                        inst.load(wasm_runtime.engine(), &component, wasm_runtime.linker())
                            .await
                            .map_err(ServiceError::Core)?;

                        let meta = inst.get_metadata().await.map_err(ServiceError::Core)?;
                        let schema = inst.get_preferences().await.ok();
                        (meta, schema)
                    };

                    match serde_json::to_value(&metadata)
                        .and_then(serde_json::from_value::<kani_shared::ExtensionMetadata>)
                    {
                        Ok(metadata) => {
                            let initially_enabled = if metadata.unrestricted_http {
                                0i64
                            } else {
                                1i64
                            };

                            let result = sqlx::query!(
                                "INSERT INTO sources (name, version, base_url, enabled, unrestricted_http)
                                VALUES (?, ?, ?, ?, ?)",
                                filename, metadata.version, metadata.base_url,
                                initially_enabled, metadata.unrestricted_http,
                            )
                            .execute(db)
                            .await?;

                            if let Some(schema) = schema {
                                let id = result.last_insert_rowid();
                                preference_schemas.insert(id, schema);
                            }

                            tracing::info!("Registered new source: {}", filename);
                        }
                        Err(e) => {
                            tracing::error!("Failed to convert metadata for {}: {}", filename, e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
