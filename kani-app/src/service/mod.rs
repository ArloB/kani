use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use indexmap::IndexMap;
use kani_shared::wit_types;
use ordered_float::OrderedFloat;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::{DownloadTask, DownloaderConfig, DownloaderManager};
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

mod audit;
pub mod backup;
mod categories;
mod chapters;
mod cover;
pub mod credential_migration;
pub mod dedup;
mod downloads;
pub mod email;
pub mod email_templates;
pub mod email_verification;
pub mod encryption;
pub mod export;
mod filters;
pub mod fs_browse;
pub mod import;
mod library;
mod migration;
pub mod opds;
pub mod password_policy;
pub mod password_reset;
pub mod path_migration;
pub mod pending_imports;
mod preferences;
mod progress;
mod scanlators;
pub mod sessions;
mod settings;
mod sources;
mod stats;
pub mod totp;
pub mod trackers;
pub mod webhooks;

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
    pub email_service: Arc<tokio::sync::RwLock<Option<email::EmailService>>>,
    /// Optional authenticated-encryption cipher for credential fields.
    /// Present when `KANI_SECRET_KEY` or `KANI_SECRET_KEY_FILE` is set at startup.
    pub encryption: Option<Arc<encryption::CredentialCipher>>,
    pub webhook_service: webhooks::WebhookService,
}

/// Load a credential cipher from env vars, or auto-provision one at `data_dir/secret.key`.
///
/// Priority order:
/// 1. `KANI_SECRET_KEY_FILE` — path to a file containing a 64-char hex key.
/// 2. `KANI_SECRET_KEY` — inline hex key.
/// 3. Auto-provision: read `data_dir/secret.key`, or generate and persist it on first boot.
///
/// The auto-provisioned file is written with `0o600` permissions (owner-read only on Unix).
fn load_or_provision_credential_cipher(
    data_dir: &std::path::Path,
) -> Option<encryption::CredentialCipher> {
    if let Ok(path) = std::env::var("KANI_SECRET_KEY_FILE") {
        let hex = match std::fs::read_to_string(path.trim()) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                tracing::error!("KANI_SECRET_KEY_FILE set but could not read file: {e}");
                return None;
            }
        };
        return parse_cipher_hex(&hex);
    }
    if let Ok(val) = std::env::var("KANI_SECRET_KEY") {
        return parse_cipher_hex(val.trim());
    }

    // Auto-provision path.
    let key_path = data_dir.join("secret.key");
    let hex = if key_path.exists() {
        match std::fs::read_to_string(&key_path) {
            Ok(s) => s.trim().to_string(),
            Err(e) => {
                tracing::error!("Failed to read {}: {e}", key_path.display());
                return None;
            }
        }
    } else {
        let key: [u8; 32] = rand::random();
        let hex = hex::encode(key);
        if let Err(e) = std::fs::write(&key_path, &hex) {
            tracing::error!(
                "Failed to write credential key to {}: {e}",
                key_path.display()
            );
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!(
            "Auto-provisioned credential encryption key at {}. \
             Back this file up alongside your database.",
            key_path.display()
        );
        hex
    };
    parse_cipher_hex(&hex)
}

fn parse_cipher_hex(hex: &str) -> Option<encryption::CredentialCipher> {
    match encryption::CredentialCipher::from_hex(hex) {
        Ok(c) => {
            tracing::info!("Credential encryption enabled");
            Some(c)
        }
        Err(e) => {
            tracing::error!("Invalid encryption key — credential encryption disabled: {e}");
            None
        }
    }
}

impl AppService {
    pub async fn new(data_dir: &std::path::Path) -> Result<Self> {
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

        let enc = load_or_provision_credential_cipher(data_dir);

        let mut settings = sqlx::query_as!(Settings, "SELECT flaresolverr_url, library_path, wasm_storage_path, concurrent_page_downloads, chapter_queue_size, max_retries, initial_retry_delay_ms, max_wasm_instances, auto_scan, scan_interval_minutes, scan_exclude_completed, auto_download_category_id, auto_download_category_ids, concurrent_manga_downloads, default_tracking_enabled, http_request_logging, browser_debug_logging, registration_enabled, cover_max_dimension, email_enabled, email_provider, email_provider_config, email_from_address, app_url, password_reset_enabled, email_verification_required FROM settings")
            .fetch_one(&pool)
            .await?;
        tracing::info!("Settings retrieved");
        kani_core::v8_process::set_v8_debug_logging(settings.browser_debug_logging);

        // Decrypt email_provider_config so in-memory value is always plaintext.
        if let Some(ref cipher) = enc {
            match cipher.decrypt(&settings.email_provider_config) {
                Ok(plain) => settings.email_provider_config = plain,
                Err(e) => tracing::warn!("Cannot decrypt email_provider_config on startup: {e}"),
            }
        }

        // KANI_ALLOW_REGISTRATION=false disables new user registration at startup,
        // overriding the database setting. Useful for instances exposed to the internet.
        if let Ok(val) = std::env::var("KANI_ALLOW_REGISTRATION") {
            let allow = val.trim() != "false" && val.trim() != "0";
            if !allow {
                settings.registration_enabled = false;
                tracing::info!("Registration disabled by KANI_ALLOW_REGISTRATION=false");
            }
        }

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
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                crate::tuning::WASM_EPOCH_TICK_MS,
            ));
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

        let sources = sqlx::query_as!(
            Source,
            "SELECT id, name, version, base_url, enabled, favourited, unrestricted_http \
             FROM sources WHERE enabled = 1 AND deleted_at IS NULL"
        )
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
            DownloaderConfig {
                concurrent_pages: settings.concurrent_page_downloads.try_into()?,
                concurrent_manga: settings.concurrent_manga_downloads.try_into()?,
                max_retries: settings.max_retries,
                initial_retry_delay_ms: settings.initial_retry_delay_ms,
                chapter_queue_size: settings.chapter_queue_size.try_into()?,
            },
        )
        .await
        .map_err(ServiceError::Core)?;
        tracing::info!("Downloader manager created");

        let proxy_client = kani_core::http::SmartClient::new_proxy(
            flaresolverr_url,
            global_smart_client.credentials.clone(),
            global_smart_client.solving.clone(),
            global_smart_client.host_circuits.clone(),
        )?;
        tracing::info!("Proxy client created");

        let (refresh_tx, _) =
            tokio::sync::broadcast::channel(crate::tuning::SSE_BROADCAST_CAPACITY);
        let refresh_task = Arc::new(tokio::sync::Mutex::new(None));

        let tracker_registry = TrackerRegistry::new(&pool, enc.as_ref()).await?;

        let email_svc = email::EmailService::from_settings(&settings);
        let enc = enc.map(Arc::new);
        let webhook_service = webhooks::WebhookService::new(pool.clone());

        let svc = Self {
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
            email_service: Arc::new(tokio::sync::RwLock::new(email_svc)),
            encryption: enc,
            webhook_service,
        };

        // Encrypt any plaintext credentials left over from pre-encryption installs.
        if let Err(e) = svc.migrate_credentials_to_encrypted().await {
            tracing::warn!("Credential encryption migration on startup failed: {e}");
        } else if svc.encryption.is_some() {
            tracing::info!("Credential encryption migration complete");
        }

        Ok(svc)
    }

    #[cfg(any(test, feature = "test-util"))]
    pub async fn new_for_test(pool: SqlitePool) -> Self {
        use crate::models::Settings;

        let settings = Settings {
            flaresolverr_url: String::new(),
            library_path: std::env::temp_dir(),
            wasm_storage_path: std::env::temp_dir(),
            concurrent_page_downloads: 4,
            concurrent_manga_downloads: 2,
            chapter_queue_size: 32,
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_wasm_instances: 1,
            auto_scan: false,
            scan_interval_minutes: 60,
            scan_exclude_completed: false,
            auto_download_category_id: None,
            auto_download_category_ids: "[]".to_string(),
            default_tracking_enabled: false,
            http_request_logging: false,
            browser_debug_logging: false,
            registration_enabled: true,
            cover_max_dimension: None,
            email_enabled: false,
            email_provider: String::new(),
            email_provider_config: String::new(),
            email_from_address: String::new(),
            app_url: String::new(),
            password_reset_enabled: false,
            email_verification_required: false,
        };

        let smart_client =
            kani_core::http::SmartClient::new(None).expect("SmartClient::new failed in test");
        let proxy_client =
            kani_core::http::SmartClient::new(None).expect("proxy SmartClient::new failed in test");
        let wasm_runtime = Arc::new(WasmRuntime::new(1).expect("WasmRuntime::new failed in test"));
        let downloader = DownloaderManager::new(
            smart_client.clone(),
            DownloaderConfig {
                concurrent_pages: 1,
                concurrent_manga: 1,
                max_retries: 0,
                initial_retry_delay_ms: 0,
                chapter_queue_size: 4,
            },
        )
        .await
        .expect("DownloaderManager::new failed in test");
        let tracker_registry = TrackerRegistry::new(&pool, None)
            .await
            .expect("TrackerRegistry::new failed in test");
        // Small capacity: tests have at most a couple of SSE subscribers.
        let (refresh_tx, _) = tokio::sync::broadcast::channel(16);

        Self {
            db: pool.clone(),
            wasm_runtime,
            sources: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client,
            proxy_client,
            refresh_tx,
            refresh_task: Arc::new(tokio::sync::Mutex::new(None)),
            cache: RequestCache::new(),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            tracker_registry: Arc::new(tokio::sync::RwLock::new(tracker_registry)),
            cover_retry_queue: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            email_service: Arc::new(tokio::sync::RwLock::new(None)),
            encryption: None,
            webhook_service: webhooks::WebhookService::new(pool),
        }
    }

    pub async fn rebuild_email_service(&self) {
        let settings = self.settings.read().await;
        let svc = email::EmailService::from_settings(&settings);
        *self.email_service.write().await = svc;
    }

    /// Returns a clone of the email service if email is enabled and configured.
    pub async fn mailer(&self) -> Option<email::EmailService> {
        self.email_service.read().await.clone()
    }

    /// Spawns a background task to send an email. Logs errors but never fails the caller.
    pub fn send_email_bg(&self, to: String, subject: String, html: String) {
        let svc = self.email_service.clone();
        tokio::spawn(async move {
            let guard = svc.read().await;
            if let Some(mailer) = guard.as_ref()
                && let Err(e) = mailer.send(&to, &subject, &html).await
            {
                tracing::warn!("Email send failed to {to}: {e}");
            }
        });
    }

    /// Subscribes to the broadcast channel and dispatches webhook events. Call once from main.
    pub fn spawn_webhook_listener(&self) {
        let mut rx = self.refresh_tx.subscribe();
        let wh = self.webhook_service.clone();
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    result = rx.recv() => match result {
                        Ok(event) => {
                            match event {
                                AppEvent::NewChapters {
                                    manga_id,
                                    manga_name,
                                    count,
                                    chapter_ids,
                                    chapter_names,
                                } => {
                                    wh.fire(webhooks::WebhookPayload::ChapterNew {
                                        manga_id,
                                        manga_name,
                                        chapter_count: count,
                                        chapter_ids,
                                        chapter_names,
                                    })
                                    .await;
                                }
                                AppEvent::Refresh(RefreshProgressEvent::Completed { total, failed }) => {
                                    wh.fire(webhooks::WebhookPayload::ScanCompleted {
                                        total_scanned: total,
                                        failed_count: failed,
                                    })
                                    .await;
                                }
                                _ => {}
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Webhook listener lagged by {n} events");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    },
                }
            }
        });
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

                let settings_snap = state.settings.read().await.clone();
                if !settings_snap.auto_scan {
                    continue;
                }
                let exclude_completed = settings_snap.scan_exclude_completed;
                let category_ids: Vec<i64> =
                    serde_json::from_str(&settings_snap.auto_download_category_ids)
                        .unwrap_or_default();
                drop(settings_snap);

                let category_manga_ids: std::collections::HashSet<i64> = if category_ids.is_empty()
                {
                    std::collections::HashSet::new()
                } else {
                    let placeholders = category_ids
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("?{}", i + 1))
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "SELECT DISTINCT manga_id FROM manga_categories WHERE category_id IN ({})",
                        placeholders
                    );
                    let mut q = sqlx::query_scalar::<_, i64>(&sql);
                    for id in &category_ids {
                        q = q.bind(*id);
                    }
                    q.fetch_all(&state.db)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .collect()
                };

                let manga_to_scan: Vec<(i64, bool)> = {
                    let base = "SELECT m.id, m.auto_download FROM manga m \
                                WHERE m.auto_scan = true";
                    let completed_clause = if exclude_completed {
                        " AND m.status != 1"
                    } else {
                        ""
                    };
                    let sql = format!("{base}{completed_clause}");
                    sqlx::query_as::<_, (i64, bool)>(&sql)
                        .fetch_all(&state.db)
                        .await
                        .unwrap_or_default()
                };

                for (manga_db_id, auto_download) in manga_to_scan {
                    // A manga auto-downloads if manually flagged OR in a nominated category.
                    let effective_auto_download =
                        auto_download || category_manga_ids.contains(&manga_db_id);
                    match state.scan_for_new_chapters(manga_db_id).await {
                        Ok(new_ids) if !new_ids.is_empty() => {
                            tracing::info!(
                                "Found {} new chapters for manga {}",
                                new_ids.len(),
                                manga_db_id
                            );
                            if effective_auto_download {
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

                if let Err(e) = sqlx::query!(
                    "DELETE FROM chapters WHERE is_orphaned = true AND download_status != 2"
                )
                .execute(&state.db)
                .await
                {
                    tracing::warn!("Orphan chapter cleanup failed: {}", e);
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
                    _ = tokio::time::sleep(std::time::Duration::from_secs(
                        crate::tuning::COVER_RETRY_INTERVAL_SECS,
                    )) => {}
                }

                let ids: Vec<i64> = state.cover_retry_queue.lock().await.drain().collect();
                if ids.is_empty() {
                    continue;
                }

                tracing::info!("Retrying cover downloads for {} manga", ids.len());
                for manga_id in ids {
                    match state.retry_single_cover(manga_id).await {
                        Ok(()) => tracing::info!("Cover retry succeeded for manga {manga_id}"),
                        Err(e) => {
                            tracing::debug!("Cover retry failed for manga {manga_id}: {e}");
                            state.cover_retry_queue.lock().await.insert(manga_id);
                        }
                    }
                }
            }
        });
    }

    /// Spawns a background task that proactively refreshes Cloudflare credentials before they expire.
    pub fn spawn_credential_refresh(&self) {
        let client = self.smart_client.clone();
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                crate::tuning::CREDENTIAL_REFRESH_INTERVAL_SECS,
            ));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Credential refresh task shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        client.refresh_expiring_credentials().await;
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
