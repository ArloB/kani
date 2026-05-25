use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use kani_shared::wit_types;
use ordered_float::OrderedFloat;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::{DownloadTask, DownloaderManager};
use kani_core::source_manager::SourceManager;
use kani_core::wasm::WasmRuntime;

use crate::cache::RequestCache;
use crate::error::{Result, ServiceError};
use crate::events::{AppEvent, RefreshProgressEvent};
use crate::models::{DownloadRuleRow, Settings};
use crate::utils::decode_manga_id;
use kani_shared::types::{
    ChapterFilterRow, DownloadRule, DownloadRuleKind, GlobalSearchResult, MangaList,
    MigrationPreview, MigrationResult, SearchScope, Source,
};
use trackers::TrackerRegistry;

mod categories;
mod chapters;
mod cover;
mod downloads;
mod filters;
mod library;
mod migration;
mod preferences;
mod progress;
mod scanlators;
mod settings;
mod sources;
pub mod trackers;

#[derive(Clone)]
pub struct AppService {
    pub db: SqlitePool,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub sources: Arc<tokio::sync::RwLock<HashMap<i64, Arc<SourceManager>>>>,
    pub settings: Arc<tokio::sync::RwLock<Settings>>,
    pub downloader: DownloaderManager,
    pub smart_client: kani_core::http::SmartClient,
    pub proxy_client: kani_core::http::SmartClient,
    pub refresh_tx: tokio::sync::broadcast::Sender<AppEvent>,
    pub refresh_task: Arc<tokio::sync::Mutex<Option<tokio::task::AbortHandle>>>,
    pub cache: RequestCache,
    pub shutdown_token: tokio_util::sync::CancellationToken,
    pub tracker_registry: Arc<tokio::sync::RwLock<TrackerRegistry>>,
    /// Manga IDs whose cover download failed and should be retried.
    pub cover_retry_queue: Arc<tokio::sync::Mutex<HashSet<i64>>>,
}

impl AppService {
    pub async fn new() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(20)
            .after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA synchronous=NORMAL;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000;")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA foreign_keys=ON;")
                        .execute(&mut *conn)
                        .await?;
                    Ok(())
                })
            })
            .connect("sqlite://kani.db?mode=rwc")
            .await?;

        tracing::info!("SQL Pool Created");

        sqlx::migrate!("../migrations").run(&pool).await?;

        let mut sources_map = HashMap::new();

        let mut settings = sqlx::query_as!(Settings, "SELECT flaresolverr_url, library_path, wasm_storage_path, concurrent_page_downloads, chapter_queue_size, max_retries, initial_retry_delay_ms, max_wasm_instances, auto_scan, scan_interval_minutes, concurrent_manga_downloads, default_tracking_enabled FROM settings")
            .fetch_one(&pool)
            .await?;
        tracing::info!("Settings retrieved");

        if let Ok(dir) = std::env::var("KANI_LIBRARY_DIR") {
            tracing::info!("Library path overridden by KANI_LIBRARY_DIR: {dir}");
            settings.library_path = std::path::PathBuf::from(dir);
        }

        let max_wasm_instances = settings.max_wasm_instances as u32;
        let wasm_runtime =
            Arc::new(WasmRuntime::new(max_wasm_instances).map_err(ServiceError::Core)?);

        let shutdown_token = tokio_util::sync::CancellationToken::new();

        let engine_for_ticker = wasm_runtime.engine().clone();
        let ticker_token = shutdown_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(10));
            loop {
                tokio::select! {
                    _ = ticker_token.cancelled() => {
                        tracing::info!("Epoch ticker shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        engine_for_ticker.increment_epoch();
                    }
                }
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

        let global_smart_client = kani_core::http::SmartClient::new(flaresolverr_url.clone())?;
        tracing::info!("Smart client created");

        let cache = RequestCache::new();

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

        let sources = sqlx::query_as!(Source, "SELECT * FROM sources WHERE enabled = 1")
            .fetch_all(&pool)
            .await?;

        for source in sources {
            let bytes = tokio::fs::read(
                &settings
                    .wasm_storage_path
                    .join(format!("{}.wasm", source.name)),
            )
            .await?;
            let component = wasm_runtime
                .compile_component(&bytes)
                .map_err(ServiceError::Core)?;

            let instance_pre = wasm_runtime
                .instantiate_pre(&component)
                .map_err(ServiceError::Core)?;

            let prefs = Self::load_pref_map_static(&pool, source.id).await?;

            let source_manager = SourceManager::new(
                wasm_runtime.engine().clone(),
                instance_pre,
                global_smart_client.clone(),
                Some(source.base_url),
                source.unrestricted_http,
                25,
                prefs,
            );

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
        .map_err(ServiceError::Core)?;
        tracing::info!("Downloader manager created");

        let proxy_client = kani_core::http::SmartClient::new_proxy(
            flaresolverr_url,
            global_smart_client.credentials.clone(),
            global_smart_client.solving.clone(),
        )?;
        tracing::info!("Proxy client created");

        let (refresh_tx, _) = tokio::sync::broadcast::channel(256);
        let refresh_task = Arc::new(tokio::sync::Mutex::new(None));

        let tracker_registry = TrackerRegistry::new(&pool).await?;

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: Arc::new(tokio::sync::RwLock::new(sources_map)),
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client: global_smart_client,
            proxy_client,
            refresh_tx,
            refresh_task,
            cache,
            shutdown_token,
            tracker_registry: Arc::new(tokio::sync::RwLock::new(tracker_registry)),
            cover_retry_queue: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
        })
    }

    /// Spawns the background auto-scan loop. Call once from main after construction.
    pub fn spawn_auto_scan(&self) {
        let state = self.clone();
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            loop {
                let interval_mins = state.settings.read().await.scan_interval_minutes;
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Scan task shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(interval_mins as u64 * 60)) => {}
                }

                if !state.settings.read().await.auto_scan {
                    continue;
                }

                let manga_to_scan: Vec<(i64, bool)> =
                    sqlx::query_as("SELECT id, auto_download FROM manga")
                        .fetch_all(&state.db)
                        .await
                        .unwrap_or_default();

                for (manga_db_id, auto_download) in manga_to_scan {
                    match state.scan_for_new_chapters(manga_db_id).await {
                        Ok(new_ids) if !new_ids.is_empty() => {
                            tracing::info!(
                                "Found {} new chapters for manga {}",
                                new_ids.len(),
                                manga_db_id
                            );
                            if auto_download {
                                let filtered_ids = state
                                    .filter_chapters_by_rules(manga_db_id, new_ids.clone())
                                    .await;

                                if filtered_ids.is_empty() {
                                    tracing::info!(
                                        "All new chapters for manga {} filtered out by download rules",
                                        manga_db_id
                                    );
                                } else {
                                    tracing::info!(
                                        "{} new chapters passed download rules for manga {}",
                                        filtered_ids.len(),
                                        manga_db_id
                                    );

                                    let futures = filtered_ids.into_iter().map(|new_id| {
                                        let s = state.clone();
                                        async move {
                                            match s.enqueue_claimed_chapter(new_id).await {
                                                Ok(_) => tracing::info!(
                                                    "Chapter {} enqueued for download",
                                                    new_id
                                                ),
                                                Err(e) => tracing::error!(
                                                    "Failed to enqueue chapter {}: {}",
                                                    new_id,
                                                    e
                                                ),
                                            }
                                        }
                                    });
                                    futures::future::join_all(futures).await;
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(ServiceError::NotFound(_)) => {
                            tracing::debug!(
                                "Manga {} deleted or source gone during scan, skipping",
                                manga_db_id
                            );
                        }
                        Err(e) => tracing::error!("Scan failed for manga {}: {}", manga_db_id, e),
                    }
                }

                state.retry_missing_covers().await;
            }
        });
    }

    /// Schedules a cover download retry for the given manga ID.
    /// Called from library operations when a cover download fails.
    pub async fn schedule_cover_retry(&self, manga_id: i64) {
        self.cover_retry_queue.lock().await.insert(manga_id);
    }

    /// Spawns a background task that retries failed cover downloads every 30 seconds.
    /// Only retries manga IDs that have been enqueued via `schedule_cover_retry`.
    pub fn spawn_cover_retry(&self) {
        let state = self.clone();
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Cover retry task shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
                }

                let ids: Vec<i64> = state.cover_retry_queue.lock().await.drain().collect();
                if ids.is_empty() { continue; }

                tracing::info!("Retrying cover downloads for {} manga", ids.len());
                for manga_id in ids {
                    match state.retry_single_cover(manga_id).await {
                        Ok(()) => tracing::info!("Cover retry succeeded for manga {manga_id}"),
                        Err(e) => {
                            tracing::debug!("Cover retry failed for manga {manga_id}: {e}");
                            // Re-enqueue so it is retried again next cycle.
                            state.cover_retry_queue.lock().await.insert(manga_id);
                        }
                    }
                }
            }
        });
    }

    /// Record an auditable event. Non-fatal — a DB failure only produces a warning.
    pub async fn audit(
        &self,
        user_id: Option<i64>,
        action: &str,
        target: Option<&str>,
        details: Option<serde_json::Value>,
    ) {
        let details_str = details.map(|d| d.to_string());
        if let Err(e) = sqlx::query!(
            "INSERT INTO audit_log (user_id, action, target, details) VALUES (?, ?, ?, ?)",
            user_id,
            action,
            target,
            details_str,
        )
        .execute(&self.db)
        .await
        {
            tracing::warn!("Audit log insert failed: {e}");
        }
    }

    pub fn subscribe_refresh(&self) -> tokio::sync::broadcast::Receiver<AppEvent> {
        self.refresh_tx.subscribe()
    }
}

pub fn chapter_name(volume: Option<i64>, chapter_number: f64, title: Option<String>) -> String {
    let mut name = String::new();
    if let Some(vol) = volume {
        name.push_str(&format!("Vol. {vol} "));
    }
    if chapter_number.fract().abs() < f64::EPSILON {
        name.push_str(&format!("Ch. {}", chapter_number as i64));
    } else {
        name.push_str(&format!("Ch. {chapter_number:.1}"));
    }
    if let Some(title) = title
        && !title.is_empty()
    {
        name.push_str(&format!(" - {title}"));
    }
    name
}

pub(crate) async fn cleanup_staging_dirs(library_path: &std::path::Path) -> std::io::Result<()> {
    let mut manga_dirs = tokio::fs::read_dir(library_path).await?;

    while let Ok(Some(manga_dir)) = manga_dirs.next_entry().await {
        if !manga_dir.file_type().await?.is_dir() {
            continue;
        }

        let Ok(mut inner_entries) = tokio::fs::read_dir(manga_dir.path()).await else {
            continue;
        };

        while let Ok(Some(entry)) = inner_entries.next_entry().await {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };

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

pub(crate) fn unwrap_cache_err(e: Arc<ServiceError>) -> ServiceError {
    match Arc::try_unwrap(e) {
        Ok(err) => err,
        Err(arc) => ServiceError::Internal(arc.to_string()),
    }
}

fn ext_for_content_type(ct: &str) -> &'static str {
    if ct.contains("jpeg") || ct.contains("jpg") {
        "jpg"
    } else if ct.contains("png") {
        "png"
    } else if ct.contains("webp") {
        "webp"
    } else if ct.contains("gif") {
        "gif"
    } else {
        "jpg"
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

fn match_chapters_inner(
    existing: &[(i64, f64)],
    target: &[wit_types::ChapterInfo],
) -> (Vec<(i64, String)>, Vec<i64>, Vec<wit_types::ChapterInfo>) {
    use std::collections::HashMap;

    let mut by_number: HashMap<OrderedFloat<f64>, Vec<i64>> = HashMap::new();
    for &(id, num) in existing {
        by_number.entry(OrderedFloat(num)).or_default().push(id);
    }

    let mut matched = Vec::new();
    let mut unmatched_new = Vec::new();

    for ch in target {
        match by_number
            .get_mut(&OrderedFloat(ch.number))
            .and_then(|b| b.pop())
        {
            Some(existing_id) => matched.push((existing_id, ch.id.clone())),
            None => unmatched_new.push(ch.clone()),
        }
    }

    let orphaned_ids: Vec<i64> = by_number.into_values().flatten().collect();
    (matched, orphaned_ids, unmatched_new)
}
