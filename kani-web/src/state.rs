use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use kani_shared::wit_types;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use ordered_float::OrderedFloat;

use kani_core::downloader::{DownloadTask, DownloaderManager};
use kani_core::source_manager::SourceManager;
use kani_core::wasm::WasmRuntime;

use crate::error::{Result, AppError};
use crate::events::{AppEvent, RefreshProgressEvent};
use crate::models::{DownloadRuleRow, Settings};
use crate::types::{
    ChapterFilterRow, DownloadRule, DownloadRuleKind, GlobalSearchResult, MangaList,
    MigrationPreview, MigrationResult, SearchScope, Source
};

struct MigrationContext {
    new_details: wit_types::MangaInfo,
    target_chapters: Vec<wit_types::ChapterInfo>,
    matched: Vec<(i64, String)>,
    orphaned_ids: Vec<i64>,
    unmatched_new: Vec<wit_types::ChapterInfo>,
    downloaded_orphan_ids: Vec<i64>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub sources: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<i64, std::sync::Arc<SourceManager>>>,
    >,
    pub settings: std::sync::Arc<tokio::sync::RwLock<Settings>>,
    pub downloader: DownloaderManager,
    pub smart_client: kani_core::http::SmartClient,
    pub proxy_client: kani_core::http::SmartClient,
    pub refresh_tx: tokio::sync::broadcast::Sender<AppEvent>,
    pub refresh_task: Arc<tokio::sync::Mutex<Option<tokio::task::AbortHandle>>>,
    pub cache: crate::cache::RequestCache,
    pub proxy_secret: Arc<[u8; 32]>,
    pub proxy_semaphores: Arc<DashMap<String, Arc<tokio::sync::Semaphore>>>,
    pub boot_id: String,
}

impl AppState {
    pub async fn new() -> Result<Self, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .after_connect(|conn, _meta| Box::pin(async move {
                sqlx::query("PRAGMA journal_mode=WAL;").execute(&mut *conn).await?;
                sqlx::query("PRAGMA synchronous=NORMAL;").execute(&mut *conn).await?;
                sqlx::query("PRAGMA busy_timeout=5000;").execute(&mut *conn).await?;
                Ok(())
            }))
            .connect("sqlite://kani.db?mode=rwc")
            .await?;

        tracing::info!("SQL Pool Created");

        sqlx::migrate!("../migrations").run(&pool).await?;

        let mut sources_map = std::collections::HashMap::new();

        let settings = sqlx::query_as!(Settings, "SELECT flaresolverr_url, library_path, wasm_storage_path, concurrent_page_downloads, chapter_queue_size, max_retries, initial_retry_delay_ms, max_wasm_instances, auto_scan, scan_interval_minutes, concurrent_manga_downloads FROM settings")
            .fetch_one(&pool)
            .await?;
        tracing::info!("Settings retrieved");

        let max_wasm_instances = settings.max_wasm_instances as u32;
        let wasm_runtime = Arc::new(WasmRuntime::new(max_wasm_instances).map_err(AppError::CoreError)?);

        let engine_for_ticker = wasm_runtime.engine().clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
            loop {
                interval.tick().await;
                engine_for_ticker.increment_epoch();
            }
        });

        let library_path = std::path::Path::new(&settings.library_path);
        if library_path.exists() {
            if let Err(e) = cleanup_staging_dirs(library_path).await {
                tracing::warn!("Failed to read library path for cleanup: {}", e);
            }
            tracing::info!("Library cleanup complete");
        }

        let flaresolverr_url = if settings.flaresolverr_url.is_empty() {
            None
        } else {
            Some(settings.flaresolverr_url.clone())
        };

        let global_smart_client = kani_core::http::SmartClient::new(flaresolverr_url)?;
        tracing::info!("Smart client created");

        let cache = crate::cache::RequestCache::new();

        if let Err(e) = Self::scan_and_register_sources(
            &pool,
            &settings.wasm_storage_path,
            global_smart_client.clone(),
            &wasm_runtime,
            &cache.preference_schema,
        )
        .await
        {
            tracing::error!("Failed to scan and register sources: {}", e);
        }
        tracing::info!("Sources scanned and registered");

        let sources = sqlx::query_as!(
            Source,
            "SELECT * FROM sources WHERE enabled = 1"
        )
        .fetch_all(&pool)
        .await?;

        for source in sources {
            let bytes = tokio::fs::read(
                &settings.wasm_storage_path.join(format!("{}.wasm", source.name)),
            )
            .await
            .map_err(AppError::IoError)?;
            let component = wasm_runtime
                .compile_component(&bytes)
                .map_err(AppError::CoreError)?;

            let prefs = Self::load_pref_map_static(&pool, source.id).await?;

            let source_manager = SourceManager::new(
                wasm_runtime.engine().clone(),
                component,
                wasm_runtime.linker().clone(),
                global_smart_client.clone(),
                Some(source.base_url),
                source.unrestricted_http,
                25,
                1,
                prefs,
            )
            .await
            .map_err(AppError::CoreError)?;

            sources_map.insert(source.id, Arc::new(source_manager));
        }
        tracing::info!("Sources loaded");

        let downloader = DownloaderManager::new(
            global_smart_client.clone(),
            settings.concurrent_page_downloads.try_into()?,
            settings.concurrent_manga_downloads.try_into()?,
            settings.max_retries,
            settings.initial_retry_delay_ms,
            settings.chapter_queue_size.try_into()?,
        )
        .await
        .map_err(AppError::CoreError)?;
        tracing::info!("Downloader manager created");

        let proxy_client = kani_core::http::SmartClient::new_proxy()?;
        tracing::info!("Proxy client created");

        let (refresh_tx, _) = tokio::sync::broadcast::channel(256);
        let refresh_task = Arc::new(tokio::sync::Mutex::new(None));

        

        let proxy_secret = Arc::new(crate::proxy::load_or_generate_secret());

        let proxy_semaphores = Arc::new(DashMap::new());

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: std::sync::Arc::new(tokio::sync::RwLock::new(sources_map)),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client: global_smart_client,
            proxy_client,
            refresh_tx,
            refresh_task,
            cache,
            proxy_secret,
            proxy_semaphores,
            boot_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
        })
    }

    pub async fn get_popular_manga(&self, id: i64, page: i32) -> Result<String, AppError> {
        let sources = self.sources.clone();

        self.cache
            .get_or_fetch_popular_manga(id, page, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources.get(&id).cloned()
                        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance().await?
                    .get_popular_manga(page).await?;
                serde_json::to_string(&result)
                    .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn search_manga(&self, id: i64, query: &str, page: i32) -> Result<String, AppError> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
        };

        let result = source_manager
            .lease_instance()
            .await?
            .search_manga(query, page)
            .await?;

        serde_json::to_string(&result).map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
    }

    pub async fn get_manga_details(&self, id: i64, manga_id: &str) -> Result<String, AppError> {
        let sources    = self.sources.clone();
        let manga_id_d = crate::utils::decode_manga_id(manga_id);

        self.cache
            .get_or_fetch_manga_details(id, &manga_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources.get(&id).cloned()
                        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance().await?
                    .get_manga_details(&manga_id_d).await?;
                serde_json::to_string(&convert_to_shared_manga_info(result))
                    .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    async fn sync_manga_metadata(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, manga_row_id: i64, manga: &wit_types::MangaInfo) -> Result<()> {
        for author in &manga.authors {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", author)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                SELECT ?, 'author', id FROM people WHERE name = ?",
                manga_row_id,
                author
            )
            .execute(&mut **tx)
            .await?;
        }

        for artist in &manga.artists {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", artist)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                SELECT ?, 'artist', id FROM people WHERE name = ?",
                manga_row_id,
                artist
            )
            .execute(&mut **tx)
            .await?;
        }

        for tag in &manga.tags {
            sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", tag)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
                SELECT ?, id FROM tags WHERE name = ?",
                manga_row_id,
                tag
            )
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    pub async fn save_to_library(
        &self,
        source_id: i64,
        manga_id: &str,
    ) -> Result<i64, AppError> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&source_id).cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {source_id} not found")))?
        };

        let result = source_manager
            .lease_instance().await?
            .get_manga_details(manga_id).await?;

        let manga = convert_to_shared_manga_info(result);

        let mut tx = self.db.begin().await?;
        let status: i64 = manga.status.into();

        let decoded_manga_id = crate::utils::decode_manga_id(&manga.id);

        let insert_result = sqlx::query!(
            "INSERT OR IGNORE INTO manga (source_manga_id, source_id, name, cover_url, description, status) \
             VALUES (?, ?, ?, ?, ?, ?)",
            decoded_manga_id,
            source_id,
            manga.title,
            manga.cover_url,
            manga.description,
            status
        )
        .execute(&mut *tx)
        .await?;

        let we_inserted = insert_result.rows_affected() == 1;

        let manga_row_id: i64 = sqlx::query_scalar!(
            "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
            decoded_manga_id,
            source_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if we_inserted {
            Self::sync_manga_metadata(&mut tx, manga_row_id, &manga).await?;
        }

        tx.commit().await?;

        if we_inserted {
            if let Some(ref url) = manga.cover_url {
                let base_url = sqlx::query_scalar!(
                    "SELECT base_url FROM sources WHERE id = ?", source_id
                )
                .fetch_optional(&self.db).await?.unwrap_or_default();

                if let Err(e) = self.download_and_store_cover(manga_row_id, url, &base_url).await {
                    tracing::warn!(
                        "Failed to download cover for manga {}: {} — library entry still saved",
                        manga_row_id, e
                    );
                }
            }

            let has_next_page = self.fetch_and_store_chapter_page(source_id, manga_id, manga_row_id, 1).await.unwrap_or_else(|e| {
                tracing::error!("Failed to fetch initial chapters: {}", e);
                false
            });

            if has_next_page {
                let bg_self = self.clone();
                let bg_manga_id = manga_id.to_string();
                tokio::spawn(async move {
                    bg_self.fetch_and_store_remaining_chapters(source_id, bg_manga_id, manga_row_id, 2).await;
                });
            }
        }

        Ok(manga_row_id)
    }

    pub async fn fetch_and_store_chapter_page(
        &self,
        source_id: i64,
        manga_id: &str,
        manga_row_id: i64,
        page: i32,
    ) -> Result<bool, AppError> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&source_id).cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {} not found", source_id)))?
        };
        let res = source_manager
            .lease_instance().await?
            .get_chapter_list(manga_id, page).await?;
        let json = serde_json::to_string(&res)
            .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))?;
        let chapter_list: wit_types::ChapterList = serde_json::from_str(&json)
            .map_err(|e| AppError::InternalServerError(format!("Failed to parse chapter list: {}", e)))?;

        if chapter_list.chapters.is_empty() {
            return Ok(false);
        }

        for chunk in chapter_list.chapters.chunks(100) {
            let mut query_builder = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO chapters (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at, discovered_at) "
            );

            query_builder.push_values(chunk, |mut b, chapter| {
                b.push_bind(manga_row_id)
                    .push_bind(crate::utils::decode_manga_id(&chapter.id))
                    .push_bind(chapter.title.clone())
                    .push_bind(chapter.number)
                    .push_bind(chapter.language.clone())
                    .push_bind(chapter.volume)
                    .push_bind(chapter.scanlator.clone())
                    .push_bind(chapter.date_uploaded);
                b.push("CURRENT_TIMESTAMP");
            });

            query_builder.build().execute(&self.db).await?;
        }

        Ok(chapter_list.has_next_page)
    }

    pub async fn fetch_and_store_remaining_chapters(
        &self,
        source_id: i64,
        manga_id: String,
        manga_row_id: i64,
        start_page: i32,
    ) {
        let mut page = start_page;
        loop {
            match self.fetch_and_store_chapter_page(source_id, &manga_id, manga_row_id, page).await {
                Ok(has_next_page) => {
                    if !has_next_page {
                        break;
                    }
                    page += 1;
                }
                Err(e) => {
                    tracing::error!("Failed to fetch chapter page {} for manga {}: {:?}", page, manga_id, e);
                    break;
                }
            }
        }
    }

    pub async fn get_pages(
        &self,
        id: i64,
        manga_id: &str,
        chapter_id: &str,
    ) -> Result<String, AppError> {
        let sources      = self.sources.clone();
        let manga_id_d   = crate::utils::decode_manga_id(manga_id);
        let chapter_id_d = crate::utils::decode_manga_id(chapter_id);

        self.cache
            .get_or_fetch_pages(id, &manga_id_d.clone(), &chapter_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources.get(&id).cloned()
                        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance().await?
                    .get_pages(&manga_id_d, &chapter_id_d).await?;
                serde_json::to_string(&result)
                    .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_chapter_list_paged(
        &self,
        id: i64,
        manga_id: &str,
        page: i32,
    ) -> Result<String, AppError> {
        let sources    = self.sources.clone();
        let manga_id_d = crate::utils::decode_manga_id(manga_id);

        self.cache
            .get_or_fetch_chapter_list(id, &manga_id_d.clone(), page, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources.get(&id).cloned()
                        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance().await?
                    .get_chapter_list(&manga_id_d, page).await?;
                serde_json::to_string(&result)
                    .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn refresh_manga(&self, manga_row_id: i64) -> Result<()> {
        let ids = sqlx::query!(
            "SELECT source_id, source_manga_id, s.base_url as base_url
            FROM manga m
            JOIN sources s ON m.source_id = s.id
            WHERE m.id = ?",
            manga_row_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Manga {manga_row_id} not found")))?;

        let manga_info_raw = self.get_manga_details(ids.source_id, &ids.source_manga_id).await?;
        let manga_info: wit_types::MangaInfo = serde_json::from_str(&manga_info_raw)
            .map_err(|e| AppError::InternalServerError(format!("Failed to parse manga: {}", e)))?;

        let mut tx = self.db.begin().await?;
        let status = manga_info.status as i64;

        sqlx::query!(
            "UPDATE manga SET name = ?, cover_url = ?, description = ?, status = ? WHERE id = ?",
            manga_info.title,
            manga_info.cover_url,
            manga_info.description,
            status,
            manga_row_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "DELETE FROM manga_people WHERE manga_id = ?",
            manga_row_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "DELETE FROM manga_tags WHERE manga_id = ?",
            manga_row_id
        )
        .execute(&mut *tx)
        .await?;

        Self::sync_manga_metadata(&mut tx, manga_row_id, &manga_info).await?;

        tx.commit().await?;

        if let Some(ref url) = manga_info.cover_url
        && let Err(e) = self.download_and_store_cover(manga_row_id, url, &ids.base_url).await {
            tracing::warn!(
                "Failed to refresh cover for manga {}: {}",
                manga_row_id, e
            );
        }

        let has_next_page = self.fetch_and_store_chapter_page(ids.source_id, &ids.source_manga_id, manga_row_id, 1).await.unwrap_or_else(|e| {
            tracing::error!("Failed to fetch initial chapters during refresh: {}", e);
            false
        });

        self.cache.invalidate_chapter_list_for_manga(ids.source_id, &ids.source_manga_id).await;

        if has_next_page {
            let bg_self = self.clone();
            let bg_manga_id = ids.source_manga_id.clone();
            tokio::spawn(async move {
                bg_self.fetch_and_store_remaining_chapters(ids.source_id, bg_manga_id, manga_row_id, 2).await;
            });
        }

        Ok(())
    }

    async fn insert_chapters_batch(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        manga_row_id: i64,
        chapters: &[wit_types::ChapterInfo],
    ) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        for chunk in chapters.chunks(100) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO chapters \
                (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at, discovered_at) "
            );
            qb.push_values(chunk, |mut b, ch| {
                b.push_bind(manga_row_id)
                    .push_bind(crate::utils::decode_manga_id(&ch.id))
                    .push_bind(ch.title.clone())
                    .push_bind(ch.number)
                    .push_bind(ch.language.clone())
                    .push_bind(ch.volume)
                    .push_bind(ch.scanlator.clone())
                    .push_bind(ch.date_uploaded);
                b.push("CURRENT_TIMESTAMP");
            });
            qb.push(" RETURNING id");
            let mut rows: Vec<i64> = qb.build_query_scalar().fetch_all(&mut **tx).await?;
            ids.append(&mut rows);
        }
        Ok(ids)
    }

    fn build_chapter_predicate(
        &self,
        rules: Vec<DownloadRuleKind>,
    ) -> impl Fn(&ChapterFilterRow) -> bool {
        move |chapter| {
            for axis in 0u8..3 {
                let axis_rules: Vec<_> = rules.iter()
                    .filter(|r| r.axis() == axis)
                    .collect();

                if axis_rules.is_empty() { continue; }

                let includes: Vec<_> = axis_rules.iter().filter(|r| r.is_include()).collect();
                let excludes: Vec<_> = axis_rules.iter().filter(|r| !r.is_include()).collect();

                if !includes.is_empty() && !includes.iter().any(|r| r.passes(chapter)) {
                    return false;
                }
                if !excludes.iter().all(|r| r.passes(chapter)) {
                    return false;
                }
            }
            true
        }
    }

    pub async fn filter_chapters_by_rules(
        &self,
        manga_id: i64,
        candidate_ids: Vec<i64>,
    ) -> Vec<i64> {
        let raw_rules: Vec<DownloadRuleRow> =
            sqlx::query_as!(
                DownloadRuleRow,
                "SELECT id, manga_id, rule_type, value
                 FROM download_rules
                 WHERE manga_id = ?",
                manga_id
            )
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();

        let ids_after_rules = if raw_rules.is_empty() {
            candidate_ids.clone()
        } else {
            if candidate_ids.is_empty() { return vec![]; }

            let rules: Vec<DownloadRule> = raw_rules
                .into_iter()
                .filter_map(|row| DownloadRule::try_from(row).ok())
                .collect();

            let predicate = self.build_chapter_predicate(
                rules.into_iter().map(|dr| dr.kind).collect(),
            );

            let chapter_map: HashMap<i64, ChapterFilterRow> = {
                let mut qb = sqlx::QueryBuilder::new(
                    "SELECT id, scanlator, language, name FROM chapters WHERE id IN ("
                );
                let mut sep = qb.separated(", ");
                for id in &candidate_ids { sep.push_bind(id); }
                qb.push(")");
                qb.build_query_as::<ChapterFilterRow>()
                    .fetch_all(&self.db)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|row| (row.id, row))
                    .collect()
            };

            candidate_ids
                .iter()
                .copied()
                .filter(|id| chapter_map.get(id).map(&predicate).unwrap_or(false))
                .collect()
        };

        let prefs: HashMap<String, i64> =
            sqlx::query!(
                "SELECT scanlator, priority FROM scanlator_preferences WHERE manga_id = ?",
                manga_id
            )
            .fetch_all(&self.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.scanlator, r.priority))
            .collect();

        if prefs.is_empty() {
            return ids_after_rules;
        }

        if ids_after_rules.is_empty() { return vec![]; }

        struct ChapRow { id: i64, chapter_number: f64, scanlator: Option<String> }

        let rows: Vec<ChapRow> = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT id, chapter_number, scanlator FROM chapters WHERE id IN ("
            );
            let mut sep = qb.separated(", ");
            for id in &ids_after_rules { sep.push_bind(id); }
            qb.push(")");
            qb.build_query_as::<(i64, f64, Option<String>)>()
                .fetch_all(&self.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id, chapter_number, scanlator)| ChapRow { id, chapter_number, scanlator })
                .collect()
        };

        let mut best: HashMap<ordered_float::OrderedFloat<f64>, (i64, i64)> = HashMap::new();

        for row in &rows {
            let prio = row.scanlator.as_deref()
                .and_then(|s| prefs.get(s).copied())
                .unwrap_or(-1);
            let key = ordered_float::OrderedFloat(row.chapter_number);
            best.entry(key)
                .and_modify(|(existing_id, existing_prio)| {
                    if prio > *existing_prio {
                        *existing_id = row.id;
                        *existing_prio = prio;
                    }
                })
                .or_insert((row.id, prio));
        }

        let winner_ids: std::collections::HashSet<i64> = best.values().map(|(id, _)| *id).collect();
        ids_after_rules.into_iter().filter(|id| winner_ids.contains(id)).collect()
    }

    pub async fn scan_for_new_chapters(&self, manga_row_id: i64) -> Result<Vec<i64>> {
        let ids = sqlx::query!(
            "SELECT source_id, source_manga_id FROM manga WHERE id = ?",
            manga_row_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Manga {manga_row_id} not found")))?;

        let mut tx = self.db.begin().await?;
        let mut new_chapter_ids = Vec::new();
        let mut page = 1;

        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&ids.source_id).cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {} not found", ids.source_id)))?
        };

        loop {
            let res = source_manager
                .lease_instance().await?
                .get_chapter_list(&ids.source_manga_id, page).await?;
            let json = serde_json::to_string(&res)
                .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))?;
            let chapter_list: wit_types::ChapterList = serde_json::from_str(&json)
                .map_err(|e| AppError::InternalServerError(format!("Failed to parse chapter list: {}", e)))?;

            if chapter_list.chapters.is_empty() {
                break;
            }

            let mut page_new_ids = Vec::new();
            for chunk in chapter_list.chapters.chunks(100) {
                let ids = self.insert_chapters_batch(&mut tx, manga_row_id, chunk).await?;
                page_new_ids.extend(ids);
            }

            let new_on_page = page_new_ids.len();
            new_chapter_ids.extend(page_new_ids);

            // Note: The assumption here is that pages are ordered newest-first, so hitting
            // a page with no new chapters means all earlier pages are already stored.
            if new_on_page == 0 || !chapter_list.has_next_page {
                break;
            }

            page += 1;
        }

        tx.commit().await?;

        if !new_chapter_ids.is_empty() {
            self.cache
                .invalidate_chapter_list_for_manga(ids.source_id, &ids.source_manga_id)
                .await;

            let manga_name = sqlx::query_scalar!(
                "SELECT name FROM manga WHERE id = ?", manga_row_id
            )
            .fetch_optional(&self.db)
            .await
            .unwrap_or(None)
            .unwrap_or_default();

            let _ = self.refresh_tx.send(crate::events::AppEvent::NewChapters {
                manga_id:   manga_row_id,
                manga_name,
                count:      new_chapter_ids.len(),
            });
        }

        Ok(new_chapter_ids)
    }

    // Download chapter(s)
    pub async fn build_download_task(&self, chapter_id: i64) -> Result<DownloadTask, AppError> {
        let record = sqlx::query!(
            "SELECT
                c.source_chapter_id, c.name, c.chapter_number, c.volume, c.language,
                m.id as manga_id, m.source_id, m.source_manga_id, m.name as manga_name,
                m.description, s.base_url,
                (SELECT GROUP_CONCAT(p.name, ', ')
                FROM manga_people mp JOIN people p ON mp.person_id = p.id
                WHERE mp.manga_id = m.id and role = 'author') as authors,
                (SELECT GROUP_CONCAT(p.name, ', ')
                FROM manga_people mp JOIN people p ON mp.person_id = p.id
                WHERE mp.manga_id = m.id and role = 'artist') as artists,
                (SELECT GROUP_CONCAT(t.name, ', ')
                FROM manga_tags mt JOIN tags t ON mt.tag_id = t.id
                WHERE mt.manga_id = m.id) as tags
            FROM chapters c
            JOIN manga m ON c.manga_id = m.id
            JOIN sources s ON m.source_id = s.id
            WHERE c.id = ?",
            chapter_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Chapter {chapter_id} not found (deleted after claim)"
            ))
        })?;

        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&record.source_id).cloned().ok_or_else(|| {
                AppError::NotFound(format!("Source {} not found", record.source_id))
            })?
        };

        let library_path = self.settings.read().await.library_path.clone();
        let name = chapter_name(record.volume, record.chapter_number, record.name.clone());
        let save_path = library_path.join(format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&record.manga_name),
            record.manga_id
        ));

        let comic_info = kani_core::comic_info::ComicInfo {
            xmlns_xsi:    "http://www.w3.org/2001/XMLSchema-instance",
            series:       record.manga_name,
            title:        record.name,
            number:       record.chapter_number,
            volume:       record.volume,
            summary:      record.description,
            language_iso: Some(record.language),
            writer:       record.authors,
            penciller:    record.artists,
            genre:        record.tags,
            web:          Some(format!("{}/{}", record.base_url, record.source_manga_id)),
        };

        Ok(DownloadTask {
            chapter_id,
            source_manager,
            source_manga_id:   record.source_manga_id,
            source_chapter_id: record.source_chapter_id,
            name,
            library_path,
            save_path,
            comic_info: Some(comic_info),
        })
    }

    pub async fn enqueue_claimed_chapter(&self, chapter_id: i64) -> Result<(), AppError> {
        let result = async {
            let task = self.build_download_task(chapter_id).await?;
            self.downloader
                .queue_chapter(task)
                .await
                .map_err(AppError::CoreError)?;
            Ok(())
        }
        .await;

        if result.is_err() {
            let _ = sqlx::query!(
                "UPDATE chapters SET download_status = 0 WHERE id = ?",
                chapter_id
            )
            .execute(&self.db)
            .await;
        }

        result
    }

    pub async fn download_chapter(&self, chapter_id: i64) -> Result<(), AppError> {
        let claimed = sqlx::query!(
            "UPDATE chapters SET download_status = 1 \
             WHERE id = ? AND download_status = 0",
            chapter_id
        )
        .execute(&self.db)
        .await?;

        if claimed.rows_affected() == 0 {
            return Err(AppError::InternalServerError(format!(
                "Chapter {chapter_id} is already downloaded or in progress."
            )));
        }

        self.enqueue_claimed_chapter(chapter_id).await
    }

    pub async fn download_all_chapters(&self, manga_id: i64) -> Result<(), AppError> {
        let claimed_ids = sqlx::query_scalar!(
            "UPDATE chapters SET download_status = 1 \
             WHERE manga_id = ? AND download_status = 0 \
             RETURNING id",
            manga_id
        )
        .fetch_all(&self.db)
        .await?;

        if claimed_ids.is_empty() {
            tracing::info!(
                "download_all_chapters: no undownloaded chapters for manga {}",
                manga_id
            );
            return Ok(());
        }

        let mut tasks: FuturesUnordered<_> = claimed_ids
            .into_iter()
            .map(|chapter_id| {
                let this = self.clone();
                async move { (chapter_id, this.enqueue_claimed_chapter(chapter_id).await) }
            })
            .collect();

        while let Some((chapter_id, result)) = tasks.next().await {
            if let Err(e) = result {
                tracing::error!(
                    "Failed to enqueue chapter {} (manga {}): {}",
                    chapter_id, manga_id, e
                );
            }
        }

        Ok(())
    }

    pub async fn delete_downloaded(&self, chapter_id: i64) -> Result<(), AppError> {
        let record = sqlx::query!("SELECT c.download_status, c.volume, c.chapter_number, c.name as chapter_name, m.id as manga_id, m.name as manga_name FROM chapters c join manga m on c.manga_id = m.id WHERE c.id = ?", chapter_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Chapter {chapter_id} not found")))?;

        if record.download_status != 2 {
            return Err(AppError::InternalServerError(format!("Chapter {chapter_id} is not downloaded.")));
        }

        let name = chapter_name(record.volume, record.chapter_number, record.chapter_name);

        let library_path = self.settings.read().await.library_path.clone();
        let safe_manga_name_base = kani_core::utilities::sanitize_filename(&record.manga_name);
        let safe_manga_name = format!("{} - {}", safe_manga_name_base, record.manga_id);
        let path = library_path.join(safe_manga_name);
        let safe_chapter_name = kani_core::utilities::sanitize_filename(&name);
        let cbz_path = path.join(format!("{}.cbz", &safe_chapter_name));

        if let Err(e) = tokio::fs::remove_file(&cbz_path).await {
            tracing::error!("Failed to remove chapter file: {}", e);
        }

        let _ = sqlx::query!(
            "UPDATE chapters SET download_status = 0 WHERE id = ?",
            chapter_id
        )
        .execute(&self.db)
        .await;

        Ok(())
    }

    pub async fn get_metadata(&self, id: i64) -> Result<String, AppError> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
        };

        let result = source_manager
            .lease_instance()
            .await?
            .get_metadata()
            .await?;

        serde_json::to_string(&result).map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
    }

    pub async fn global_search(
        &self,
        query: &str,
        scope: SearchScope,
        page: i32,
    ) -> Result<Vec<GlobalSearchResult>, AppError> {
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
                    let result = state.search_manga(source_id, &q, page).await;
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
                            return Err(AppError::CoreError(kani_core::Error::Json(e)));
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

    async fn scan_and_register_sources(
        db: &SqlitePool,
        wasm_storage_path: &std::path::Path,
        smart_client: kani_core::http::SmartClient,
        wasm_runtime: &WasmRuntime,
        preference_schemas: &DashMap<i64, Vec<crate::types::PreferenceDescriptor>>,
    ) -> Result<(), AppError> {
        tracing::info!(
            "Scanning and registering sources in {:?}",
            wasm_storage_path
        );

        let mut entries = tokio::fs::read_dir(wasm_storage_path)
            .await
            .map_err(|e| AppError::CoreError(kani_core::Error::Io(e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::CoreError(kani_core::Error::Io(e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| AppError::InternalServerError("Invalid filename".to_string()))?;

                let exists = sqlx::query!("SELECT id FROM sources WHERE name = ?", filename)
                    .fetch_optional(db)
                    .await?
                    .is_some();

                if !exists {
                    let bytes = tokio::fs::read(&path)
                        .await
                        .map_err(|e| AppError::CoreError(kani_core::Error::Io(e)))?;

                    let component = wasm_runtime
                        .compile_component(&bytes)
                        .map_err(AppError::CoreError)?;

                    let (metadata, schema) = {
                        let mut inst = kani_core::sources::SourceInstance::new(
                            smart_client.clone(), None, false
                        );
                        inst.load(wasm_runtime.engine(), &component, wasm_runtime.linker())
                            .await
                            .map_err(AppError::CoreError)?;

                        let meta = inst.get_metadata().await.map_err(AppError::CoreError)?;
                        let schema = inst.get_preferences().await.ok();
                        (meta, schema)
                    };

                    match serde_json::to_value(&metadata)
                        .and_then(serde_json::from_value::<kani_shared::ExtensionMetadata>)
                    {
                        Ok(metadata) => {
                            let initially_enabled = if metadata.unrestricted_http { 0i64 } else { 1i64 };

                            let result = sqlx::query!(
                                "INSERT INTO sources (name, version, base_url, enabled, unrestricted_http)
                                VALUES (?, ?, ?, ?, ?)",
                                filename, metadata.version, metadata.base_url,
                                initially_enabled, metadata.unrestricted_http,
                            )
                            .execute(db)
                            .await
                            .map_err(AppError::SqlxError)?;

                            if let Some(raw_schema) = schema {
                                let id = result.last_insert_rowid();
                                let converted: Vec<_> = raw_schema.into_iter().map(Into::into).collect();
                                preference_schemas.insert(id, converted);
                            }

                            tracing::info!("Registered new source: {}", filename);
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to convert metadata for {}: {}",
                                filename,
                                e
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn start_refresh_all(&self) -> Result<(), AppError> {
        let mut task_guard = self.refresh_task.lock().await;

        if let Some(handle) = &*task_guard
            && !handle.is_finished() {
                return Err(AppError::InternalServerError("Refresh already in progress".into()));
            }

        let ids: Vec<(i64, String)> = sqlx::query!(
            "SELECT id, name FROM manga ORDER BY id"
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();

        let total = ids.len();
        let state = self.clone();

        let handle = tokio::spawn(async move {
            let _ = state.refresh_tx.send(AppEvent::Refresh(RefreshProgressEvent::Started { total }));

            let mut futures = FuturesUnordered::new();
            for (id, name) in ids {
                let s = state.clone();
                let n = name.clone();
                futures.push(async move {
                    let success = s.refresh_manga(id).await.is_ok();
                    (id, n, success)
                });
            }

            let mut completed = 0usize;
            let mut failed = 0usize;

            while let Some((manga_id, manga_name, success)) = futures.next().await {
                completed += 1;
                if !success { failed += 1; }

                let _ = state.refresh_tx.send(AppEvent::Refresh(RefreshProgressEvent::MangaRefreshed {
                    manga_id,
                    manga_name,
                    completed,
                    total,
                    success,
                }));
            }

            let _ = state.refresh_tx.send(AppEvent::Refresh(RefreshProgressEvent::Completed { total, failed }));

            *state.refresh_task.lock().await = None;
        })
        .abort_handle();

        *task_guard = Some(handle);
        Ok(())
    }

    pub fn subscribe_refresh(&self) -> tokio::sync::broadcast::Receiver<AppEvent> {
        self.refresh_tx.subscribe()
    }

    pub async fn is_refreshing(&self) -> bool {
        self.refresh_task
            .lock()
            .await
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }

    async fn download_and_store_cover(
        &self,
        manga_row_id: i64,
        cover_url: &str,
        base_url: &str,
    ) -> Result<(), AppError> {
        let library_path = self.settings.read().await.library_path.clone();
        let covers_dir   = library_path.join("covers");
        tokio::fs::create_dir_all(&covers_dir).await
            .map_err(AppError::IoError)?;

        let mut headers = rquest::header::HeaderMap::new();
        if let Ok(v) = rquest::header::HeaderValue::from_str(base_url) {
            headers.insert(rquest::header::REFERER, v);
        }

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.proxy_client.safe_get(cover_url, Some(headers)),
        )
        .await
        .map_err(|_| AppError::Other("Cover download timed out".into()))??;

        if !response.status().is_success() {
            return Err(AppError::Other(format!(
                "Cover download returned {}",
                response.status().as_u16()
            )));
        }

        let content_type = response
            .headers()
            .get(rquest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        if !content_type.starts_with("image/") {
            return Err(AppError::Other(format!(
                "Expected image for cover, got Content-Type: {}",
                content_type
            )));
        }

        let ext = ext_for_content_type(&content_type);

        const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;
        let bytes = response
            .bytes_limited(MAX_COVER_BYTES)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;

        let filename    = format!("{}.{}", manga_row_id, ext);
        let cover_path  = covers_dir.join(&filename);
        let relative    = format!("covers/{}", filename);

        tokio::fs::write(&cover_path, &bytes).await
            .map_err(AppError::IoError)?;

        sqlx::query!(
            "UPDATE manga SET local_cover_path = ? WHERE id = ?",
            relative,
            manga_row_id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn get_source(&self, id: i64) -> Result<Source, AppError> {
        let source = sqlx::query_as!(
            Source,
            "SELECT * FROM sources WHERE id = ?",
            id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Source not found".into()))?;

        Ok(source)
    }

    pub async fn get_preference(&self, source_id: i64, key: &str) -> Result<Option<String>, AppError> {
        sqlx::query_scalar!("SELECT value FROM source_preferences WHERE source_id = ? AND key = ?", source_id, key)
            .fetch_optional(&self.db).await.map_err(Into::into)
    }

    pub async fn set_preference(&self, source_id: i64, key: &str, value: &str) -> Result<(), AppError> {
        sqlx::query!("INSERT INTO source_preferences (source_id, key, value) VALUES (?, ?, ?) ON CONFLICT (source_id, key) DO UPDATE SET value = excluded.value", source_id, key, value)
            .execute(&self.db).await?;
        self.reload_preferences(source_id).await
    }

    pub async fn reload_preferences(&self, source_id: i64) -> Result<(), AppError> {
        let prefs = self.load_pref_map(source_id).await?;
        if let Some(mgr) = self.sources.read().await.get(&source_id) {
            mgr.update_preferences(prefs);
        }
        Ok(())
    }


    pub async fn load_pref_map(&self, source_id: i64) -> Result<HashMap<String, String>, AppError> {
        Self::load_pref_map_static(&self.db, source_id).await
    }

    async fn load_pref_map_static(db: &sqlx::Pool<sqlx::Sqlite>, source_id: i64) -> Result<HashMap<String, String>, AppError> {
        let raw = sqlx::query!(
            "SELECT key, value FROM source_preferences WHERE source_id = ?",
            source_id
        )
        .fetch_all(db)
        .await?;

        let mut map = HashMap::new();
        for row in raw {
            map.insert(row.key, row.value);
        }

        Ok(map)
    }

    async fn fetch_all_chapter_pages(
        &self,
        source_id: i64,
        source_manga_id: &str,
    ) -> Result<Vec<wit_types::ChapterInfo>, AppError> {
        let mut all: Vec<wit_types::ChapterInfo> = Vec::new();
        let mut page = 1i32;
        loop {
            let raw  = self.get_chapter_list_paged(source_id, source_manga_id, page).await?;
            let list: wit_types::ChapterList = serde_json::from_str(&raw)
                .map_err(|e| AppError::InternalServerError(
                    format!("Failed to parse chapter list: {e}")
                ))?;
            all.extend(list.chapters);
            if !list.has_next_page { break; }
            page += 1;
        }
        Ok(all)
    }

    async fn resolve_migration_context(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: &str,
    ) -> Result<MigrationContext, AppError> {
        {
            let sources = self.sources.read().await;
            if sources.get(&target_source_id).is_none() {
                return Err(AppError::NotFound(
                    format!("Source {target_source_id} not found")
                ));
            }
        }

        let conflict = sqlx::query_scalar!(
            "SELECT id FROM manga WHERE source_id = ? AND source_manga_id = ?",
            target_source_id, target_source_manga_id
        )
        .fetch_optional(&self.db)
        .await?;

        if conflict.is_some() {
            return Err(AppError::Conflict(
                "Target manga is already in your library from this source".to_string()
            ));
        }

        let raw = self
            .get_manga_details(target_source_id, target_source_manga_id)
            .await?;
        let new_details: wit_types::MangaInfo = serde_json::from_str(&raw)
            .map_err(|e| AppError::InternalServerError(
                format!("Failed to parse manga details: {e}")
            ))?;

        let target_chapters = self
            .fetch_all_chapter_pages(target_source_id, target_source_manga_id)
            .await?;

        let existing_chapters = sqlx::query!(
            "SELECT id, chapter_number, download_status FROM chapters WHERE manga_id = ?",
            manga_db_id
        )
        .fetch_all(&self.db)
        .await?;

        let existing_pairs: Vec<(i64, f64)> = existing_chapters
            .iter()
            .map(|c| (c.id, c.chapter_number))
            .collect();
        let (matched, orphaned_ids, unmatched_new) =
            match_chapters_inner(&existing_pairs, &target_chapters);

        let downloaded_orphan_ids: Vec<i64> = existing_chapters
            .iter()
            .filter(|c| orphaned_ids.contains(&c.id) && c.download_status == 2)
            .map(|c| c.id)
            .collect();

        Ok(MigrationContext {
            new_details,
            target_chapters,
            matched,
            orphaned_ids,
            unmatched_new,
            downloaded_orphan_ids,
        })
    }
    
    pub async fn preview_migration(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: String,
    ) -> Result<MigrationPreview, AppError> {
        let ctx = self
            .resolve_migration_context(manga_db_id, target_source_id, &target_source_manga_id)
            .await?;

        Ok(MigrationPreview {
            target_title:               ctx.new_details.title,
            target_cover_url:           ctx.new_details.cover_url,
            chapters_matched:           ctx.matched.len(),
            chapters_orphaned:          ctx.orphaned_ids.len(),
            chapters_new:               ctx.unmatched_new.len(),
            downloaded_chapters_at_risk: ctx.downloaded_orphan_ids.len(),
        })
    }

    pub async fn migrate_manga(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: String,
    ) -> Result<MigrationResult, AppError> {
        let old_manga = sqlx::query!("SELECT name FROM manga WHERE id = ?", manga_db_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Manga {manga_db_id} not found")))?;
        let old_manga_name = old_manga.name;

        let ctx = self
            .resolve_migration_context(manga_db_id, target_source_id, &target_source_manga_id)
            .await?;

        let MigrationContext {
            new_details,
            target_chapters,
            matched,
            orphaned_ids,
            unmatched_new,
            downloaded_orphan_ids,
        } = ctx;

        let new_count = unmatched_new.len();

        let library_path = self.settings.read().await.library_path.clone();
        let old_dir_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&old_manga_name),
            manga_db_id
        );
        let new_dir_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&new_details.title),
            manga_db_id
        );

        for orphan_id in &downloaded_orphan_ids {
            let ch = sqlx::query!(
                "SELECT name, chapter_number, volume FROM chapters WHERE id = ?",
                orphan_id
            )
            .fetch_optional(&self.db)
            .await?;

            if let Some(ch) = ch {
                let ch_name  = chapter_name(ch.volume, ch.chapter_number, ch.name);
                let cbz_path = library_path
                    .join(&old_dir_name)
                    .join(format!(
                        "{}.cbz",
                        kani_core::utilities::sanitize_filename(&ch_name)
                    ));
                if cbz_path.exists()
                && let Err(e) = tokio::fs::remove_file(&cbz_path).await {
                    tracing::warn!(
                        "Failed to delete orphaned CBZ {:?}: {}", cbz_path, e
                    );
                }
            }
        }

        let mut tx     = self.db.begin().await?;
        let status: i64 = new_details.status.into();

        sqlx::query!(
            "UPDATE manga SET source_id = ?, source_manga_id = ?, name = ?,
            cover_url = ?, description = ?, status = ? WHERE id = ?",
            target_source_id, target_source_manga_id,
            new_details.title, new_details.cover_url, new_details.description,
            status, manga_db_id
        )
        .execute(&mut *tx)
        .await?;

        for (existing_id, new_source_chapter_id) in &matched {
            let target_ch = target_chapters
                .iter()
                .find(|c| c.id == *new_source_chapter_id)
                .expect("matched chapter must be present in target_chapters");

            let vol: Option<i64> = target_ch.volume.map(|v| v as i64);
            sqlx::query!(
                "UPDATE chapters SET source_chapter_id = ?, name = ?, language = ?,
                scanlator = ?, uploaded_at = ?, volume = ? WHERE id = ?",
                new_source_chapter_id,
                target_ch.title, target_ch.language, target_ch.scanlator,
                target_ch.date_uploaded, vol,
                existing_id
            )
            .execute(&mut *tx)
            .await?;
        }

        for orphan_id in &orphaned_ids {
            sqlx::query!("DELETE FROM chapters WHERE id = ?", orphan_id)
                .execute(&mut *tx)
                .await?;
        }

        for ch in &unmatched_new {
            let vol: Option<i64> = ch.volume.map(|v| v as i64);
            sqlx::query!(
                "INSERT OR IGNORE INTO chapters
                (manga_id, source_chapter_id, name, chapter_number, language,
                volume, scanlator, uploaded_at, discovered_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)",
                manga_db_id, ch.id, ch.title, ch.number, ch.language,
                vol, ch.scanlator, ch.date_uploaded
            )
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query!("DELETE FROM manga_people WHERE manga_id = ?", manga_db_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM manga_tags WHERE manga_id = ?", manga_db_id)
            .execute(&mut *tx)
            .await?;
        Self::sync_manga_metadata(&mut tx, manga_db_id, &new_details).await?;

        tx.commit().await?;

        if old_dir_name != new_dir_name {
            let old_path = library_path.join(&old_dir_name);
            let new_path = library_path.join(&new_dir_name);
            if old_path.exists()
            && let Err(e) = tokio::fs::rename(&old_path, &new_path).await {
                tracing::warn!(
                    "Failed to rename library directory {:?} → {:?}: {}",
                    old_path, new_path, e
                );
            }
        }

        Ok(MigrationResult {
            chapters_matched:  matched.len(),
            chapters_orphaned: orphaned_ids.len(),
            chapters_new:      new_count,
        })
    }
}

fn convert_to_shared_manga_info(
    info: kani_core::wasm::kani::extension::types::MangaInfo,
) -> wit_types::MangaInfo {
    use kani_core::wasm::kani::extension::types::MangaStatus as CoreMangaStatus;
    use kani_shared::MangaStatus as SharedMangaStatus;

    let status = match info.status {
        CoreMangaStatus::Ongoing => SharedMangaStatus::Ongoing,
        CoreMangaStatus::Completed => SharedMangaStatus::Completed,
        CoreMangaStatus::Hiatus => SharedMangaStatus::Hiatus,
        CoreMangaStatus::Cancelled => SharedMangaStatus::Cancelled,
        CoreMangaStatus::Unknown => SharedMangaStatus::Unknown,
    };

    wit_types::MangaInfo {
        id: info.id,
        title: info.title,
        cover_url: info.cover_url,
        description: info.description,
        authors: info.authors,
        artists: info.artists,
        status,
        tags: info.tags,
    }
}

pub fn chapter_name(volume: Option<i64>, chapter_number: f64, title: Option<String>) -> String {
    let mut name = String::new();
    if let Some(vol) = volume {
        name.push_str(&format!("Vol. {vol} "));
    }
    name.push_str(&format!("Ch. {chapter_number}"));
    if let Some(title) = title
        && !title.is_empty() {
            name.push_str(&format!(" - {title}"));
        }
    name
}

async fn cleanup_staging_dirs(library_path: &std::path::Path) -> std::io::Result<()> {
    let mut manga_dirs = tokio::fs::read_dir(library_path).await?;

    while let Ok(Some(manga_dir)) = manga_dirs.next_entry().await {
        if !manga_dir.file_type().await?.is_dir() {
            continue;
        }

        let Ok(mut inner_entries) = tokio::fs::read_dir(manga_dir.path()).await else { continue };

        while let Ok(Some(entry)) = inner_entries.next_entry().await {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else { continue };

            if !entry.file_type().await?.is_dir() || !name.starts_with(".tmp_staging_") {
                continue;
            }

            tracing::info!("Removing orphaned directory: {:?}", entry.path());
            if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
                tracing::warn!("Failed to remove {:?}: {}", entry.path(), e);
            }
        }
    }

    Ok(())
}

pub(crate) fn unwrap_cache_err(e: Arc<AppError>) -> AppError {
    match Arc::try_unwrap(e) {
        Ok(err) => err,
        Err(arc) => AppError::InternalServerError(arc.to_string()),
    }
}

fn ext_for_content_type(ct: &str) -> &'static str {
    if ct.contains("jpeg") || ct.contains("jpg") { "jpg"  }
    else if ct.contains("png")                   { "png"  }
    else if ct.contains("webp")                  { "webp" }
    else if ct.contains("gif")                   { "gif"  }
    else                                         { "jpg"  }
}

fn match_chapters_inner(
    existing: &[(i64, f64)],
    target: &[wit_types::ChapterInfo],
) -> (Vec<(i64, String)>, Vec<i64>, Vec<wit_types::ChapterInfo>) {
    use std::collections::HashMap;

    let mut by_number: HashMap<OrderedFloat<f64>, Vec<i64>> = HashMap::new();
    for &(id, num) in existing {
        by_number.entry(OrderedFloat(num)).or_default().push(id);
    }

    let mut matched      = Vec::new();
    let mut unmatched_new = Vec::new();

    for ch in target {
        match by_number
            .get_mut(&OrderedFloat(ch.number))
            .and_then(|b| b.pop())
        {
            Some(existing_id) => matched.push((existing_id, ch.id.clone())),
            None              => unmatched_new.push(ch.clone()),
        }
    }

    let orphaned: Vec<i64> = by_number.into_values().flatten().collect();
    (matched, orphaned, unmatched_new)
}