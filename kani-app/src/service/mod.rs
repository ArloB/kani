use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use indexmap::IndexMap;
use kani_shared::wit_types;
use ordered_float::OrderedFloat;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::{DownloadTask, DownloaderConfig, DownloaderManager};
use kani_core::wasm::WasmRuntime;

use crate::source::{SourceRegistry, loader};

use crate::cache::RequestCache;
use crate::error::{Result, ServiceError};
use crate::events::{AppEvent, RefreshProgressEvent};
use crate::ids::MangaId;
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
pub mod metadata_provider;
mod migration;
pub mod opds;
pub mod password_policy;
pub mod password_reset;
pub mod path_migration;
pub mod pending_imports;
mod preferences;
mod progress;
pub mod repos;
mod scanlators;
pub mod sessions;
mod settings;
mod sources;
mod stats;
pub mod streaming;
pub mod totp;
pub mod trackers;
pub mod traits;
pub mod webhooks;

#[derive(Clone)]
pub struct AppService {
    pub db: SqlitePool,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub sources: Arc<SourceRegistry>,
    pub settings: Arc<tokio::sync::RwLock<Settings>>,
    pub downloader: DownloaderManager,
    pub smart_client: kani_core::http::SmartClient,
    pub proxy_client: kani_core::http::SmartClient,
    pub refresh_tx: tokio::sync::broadcast::Sender<AppEvent>,
    pub refresh_task: Arc<tokio::sync::Mutex<Option<tokio::task::AbortHandle>>>,
    pub cache: RequestCache,
    pub ext_cache: std::sync::Arc<dyn kani_core::cache::CacheBackend>,
    pub shutdown_token: tokio_util::sync::CancellationToken,
    pub tracker_registry: Arc<tokio::sync::RwLock<TrackerRegistry>>,
    /// Manga IDs whose cover download failed and should be retried.
    pub cover_retry_queue: Arc<tokio::sync::Mutex<HashSet<MangaId>>>,
    pub email_service: Arc<tokio::sync::RwLock<Option<email::EmailService>>>,
    /// Optional authenticated-encryption cipher for credential fields.
    /// Present when `KANI_SECRET_KEY` or `KANI_SECRET_KEY_FILE` is set at startup.
    pub encryption: Option<Arc<encryption::CredentialCipher>>,
    pub webhook_service: webhooks::WebhookService,
    pub metadata_provider_registry:
        Arc<tokio::sync::RwLock<metadata_provider::MetadataProviderRegistry>>,
    pub job_manager: crate::jobs::JobManager,
    /// Per-extension install/update locks, keyed by extension id. Serializes
    /// concurrent install/update of the same extension so the upsert (which keys on
    /// `sources.name`) cannot race two writers into duplicate rows + backends.
    pub install_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    #[cfg(any(test, feature = "test-util"))]
    pub mock_sources: Arc<DashMap<i64, Arc<dyn kani_core::downloader::PageListFetcher>>>,
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

        let sources_registry = SourceRegistry::new();

        let enc = load_or_provision_credential_cipher(data_dir);

        let mut settings = sqlx::query_as!(Settings, "SELECT flaresolverr_url, library_path, wasm_storage_path, concurrent_page_downloads, chapter_queue_size, max_retries, initial_retry_delay_ms, max_wasm_instances, auto_scan, scan_interval_minutes, scan_exclude_completed, auto_download_category_id, auto_download_category_ids, concurrent_manga_downloads, default_tracking_enabled, http_request_logging, browser_debug_logging, registration_enabled, cover_max_dimension, email_enabled, email_provider, email_provider_config, email_from_address, app_url, password_reset_enabled, email_verification_required, first_run_complete, scan_concurrency, per_source_download_concurrency, job_max_history, job_shutdown_timeout_secs FROM settings")
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

        // On first boot only, apply env var overrides for concurrency settings and
        // persist them so subsequent boots use the DB value (precedence: DB > env > tuning).
        if !settings.first_run_complete {
            let mut concurrency_changed = false;
            if let Ok(val) = std::env::var("KANI_SCAN_CONCURRENCY")
                && let Ok(n) = val.trim().parse::<i64>()
                && (1..=32).contains(&n)
            {
                tracing::info!("scan_concurrency set to {n} by KANI_SCAN_CONCURRENCY");
                settings.scan_concurrency = n;
                concurrency_changed = true;
            }
            if let Ok(val) = std::env::var("KANI_PER_SOURCE_DOWNLOAD_CONCURRENCY")
                && let Ok(n) = val.trim().parse::<i64>()
                && (1..=16).contains(&n)
            {
                tracing::info!(
                    "per_source_download_concurrency set to {n} by KANI_PER_SOURCE_DOWNLOAD_CONCURRENCY"
                );
                settings.per_source_download_concurrency = n;
                concurrency_changed = true;
            }
            if concurrency_changed {
                sqlx::query!(
                    "UPDATE settings SET scan_concurrency = ?, per_source_download_concurrency = ? WHERE id = 'singleton'",
                    settings.scan_concurrency,
                    settings.per_source_download_concurrency,
                )
                .execute(&pool)
                .await?;
            }
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
            if let Err(e) = cleanup_staging_dirs(library_path, &pool).await {
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
        let ext_cache: std::sync::Arc<dyn kani_core::cache::CacheBackend> =
            std::sync::Arc::new(crate::cache::SqliteCache::new(pool.clone()));

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
            "SELECT id, name, version, base_url, enabled, favourited, unrestricted_http, \
             download_concurrency, icon, description, languages, schema_version, \
             CAST(NULL AS TEXT) as circuit_state \
             FROM sources WHERE enabled = 1 AND deleted_at IS NULL"
        )
        .fetch_all(&pool)
        .await?;

        for source in sources {
            let prefs = Self::load_pref_map_static(&pool, source.id).await?;
            let ns = format!("{}:", source.name);

            let yaml_path = settings
                .wasm_storage_path
                .join(format!("{}.yaml", source.name));
            let wasm_path = settings
                .wasm_storage_path
                .join(format!("{}.wasm", source.name));

            let backend = if yaml_path.exists() {
                let text = tokio::fs::read_to_string(&yaml_path).await?;
                match kani_yaml::parse_and_validate(&text, &yaml_path) {
                    Ok(ext) => {
                        if wasm_path.exists() {
                            tracing::info!(
                                "YAML supersedes WASM for source '{}'; WASM retained for rollback",
                                source.name
                            );
                        }
                        loader::build_yaml_source(
                            std::sync::Arc::new(ext),
                            global_smart_client.clone(),
                            std::sync::Arc::clone(&ext_cache),
                            ns,
                            prefs,
                        )
                    }
                    Err(errs) => {
                        let msg = errs
                            .iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join("; ");
                        tracing::error!("Failed to load YAML source '{}': {}", source.name, msg);
                        continue;
                    }
                }
            } else {
                let bytes = tokio::fs::read(&wasm_path).await?;
                let component = wasm_runtime
                    .compile_component(&bytes)
                    .map_err(ServiceError::Core)?;

                let instance_pre = wasm_runtime
                    .instantiate_pre(&component)
                    .map_err(ServiceError::Core)?;

                let (pure_registry, hook_registry, max_hook_requests) = {
                    let mut inst = kani_core::sources::SourceInstance::new(
                        global_smart_client.clone(),
                        None,
                        false,
                    );
                    if inst
                        .load(wasm_runtime.engine(), &component, wasm_runtime.linker())
                        .await
                        .is_ok()
                    {
                        let meta = inst.get_metadata().await.ok().and_then(|raw| {
                            serde_json::from_str::<kani_shared::ExtensionMetadata>(&raw).ok()
                        });
                        let max_hk = meta
                            .as_ref()
                            .and_then(|m| m.rate_limit.as_ref())
                            .map(|rl| rl.max_hook_requests)
                            .unwrap_or(3);
                        let pure_reg = meta.as_ref().and_then(|m| {
                            if m.scripts.is_empty() {
                                return None;
                            }
                            match kani_core::scripting::PureFunctionRegistry::compile(&m.scripts) {
                                Ok(reg) => Some(Arc::new(reg)),
                                Err(e) => {
                                    tracing::warn!(
                                        source = %source.name,
                                        "Failed to compile pure scripts: {e}"
                                    );
                                    None
                                }
                            }
                        });
                        let hook_reg = meta.as_ref().and_then(sources::compile_hook_registry);
                        (pure_reg, hook_reg, max_hk)
                    } else {
                        (None, None, 3u32)
                    }
                };

                loader::build_wasm_source(
                    wasm_runtime.engine().clone(),
                    instance_pre,
                    global_smart_client.clone(),
                    Some(source.base_url),
                    source.unrestricted_http,
                    prefs,
                    std::sync::Arc::clone(&ext_cache),
                    ns,
                    pure_registry,
                    hook_registry,
                    max_hook_requests,
                )
            };

            sources_registry.insert(source.id, backend);
        }
        tracing::info!("Sources loaded");

        let downloader = DownloaderManager::new(
            global_smart_client.clone(),
            DownloaderConfig {
                concurrent_pages: settings.concurrent_page_downloads.try_into()?,
                max_attempts: settings.max_retries,
                initial_retry_delay_ms: settings.initial_retry_delay_ms,
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

        let svc_cell: crate::jobs::framework::ServiceCell = Arc::new(std::sync::Mutex::new(None));

        let mut job_registry = crate::jobs::JobRegistry::new();
        job_registry.register::<crate::jobs::download::ChapterDownloadJob>();
        job_registry.register::<crate::jobs::download::MangaDownloadAllJob>();
        job_registry.register::<crate::jobs::download::SourceScanJob>();
        job_registry.register::<crate::jobs::download::LibraryScanJob>();

        let job_manager = crate::jobs::JobManager::new(
            pool.clone(),
            refresh_tx.clone(),
            shutdown_token.clone(),
            crate::jobs::JobManagerConfig {
                global_max_concurrent: crate::tuning::DEFAULT_MAX_CONCURRENT_JOBS,
                job_shutdown_timeout: std::time::Duration::from_secs(
                    settings
                        .job_shutdown_timeout_secs
                        .try_into()
                        .unwrap_or(crate::tuning::DEFAULT_JOB_SHUTDOWN_TIMEOUT_SECS),
                ),
                type_configs: std::collections::HashMap::new(),
                registry: job_registry,
                max_history: settings
                    .job_max_history
                    .try_into()
                    .unwrap_or(crate::tuning::DEFAULT_JOB_MAX_HISTORY),
                concurrency: crate::jobs::ConcurrencyConfig {
                    page_concurrency: settings.concurrent_page_downloads.try_into()?,
                    per_source_download_concurrency: settings
                        .per_source_download_concurrency
                        .try_into()?,
                    scan_concurrency: settings.scan_concurrency.try_into()?,
                },
                svc_cell: Arc::clone(&svc_cell),
            },
        )
        .await?;
        tracing::info!("Job manager created");

        let svc = Self {
            db: pool,
            wasm_runtime,
            sources: Arc::new(sources_registry),
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client: global_smart_client,
            proxy_client,
            refresh_tx,
            refresh_task,
            cache,
            ext_cache,
            shutdown_token,
            tracker_registry: Arc::new(tokio::sync::RwLock::new(tracker_registry)),
            cover_retry_queue: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            email_service: Arc::new(tokio::sync::RwLock::new(email_svc)),
            encryption: enc,
            webhook_service,
            metadata_provider_registry: Arc::new(tokio::sync::RwLock::new(
                metadata_provider::MetadataProviderRegistry::new(),
            )),
            job_manager,
            install_locks: Arc::new(DashMap::new()),
            #[cfg(any(test, feature = "test-util"))]
            mock_sources: Arc::new(DashMap::new()),
        };

        *svc_cell.lock().expect("svc_cell lock") = Some(svc.clone());

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
            first_run_complete: false,
            scan_concurrency: crate::tuning::DEFAULT_SCAN_CONCURRENCY as i64,
            per_source_download_concurrency: crate::tuning::DEFAULT_PER_SOURCE_DOWNLOAD_CONCURRENCY
                as i64,
            job_max_history: crate::tuning::DEFAULT_JOB_MAX_HISTORY as i64,
            job_shutdown_timeout_secs: crate::tuning::DEFAULT_JOB_SHUTDOWN_TIMEOUT_SECS as i64,
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
                max_attempts: 0,
                initial_retry_delay_ms: 0,
            },
        )
        .await
        .expect("DownloaderManager::new failed in test");
        let tracker_registry = TrackerRegistry::new(&pool, None)
            .await
            .expect("TrackerRegistry::new failed in test");
        // Small capacity: tests have at most a couple of SSE subscribers.
        let (refresh_tx, _) = tokio::sync::broadcast::channel(16);
        let shutdown_token = tokio_util::sync::CancellationToken::new();

        let svc_cell: crate::jobs::framework::ServiceCell = Arc::new(std::sync::Mutex::new(None));

        let mut registry = crate::jobs::JobRegistry::new();
        registry.register::<crate::jobs::test_jobs::TestJob>();
        registry.register::<crate::jobs::test_jobs::SlowTestJob>();
        registry.register::<crate::jobs::test_jobs::FailingDownloadJob>();
        registry.register::<crate::jobs::download::ChapterDownloadJob>();
        registry.register::<crate::jobs::download::MangaDownloadAllJob>();
        registry.register::<crate::jobs::download::SourceScanJob>();
        registry.register::<crate::jobs::download::LibraryScanJob>();

        let job_manager = crate::jobs::JobManager::new(
            pool.clone(),
            refresh_tx.clone(),
            shutdown_token.clone(),
            crate::jobs::JobManagerConfig {
                global_max_concurrent: crate::tuning::DEFAULT_MAX_CONCURRENT_JOBS,
                job_shutdown_timeout: std::time::Duration::from_secs(5),
                type_configs: std::collections::HashMap::new(),
                registry,
                max_history: crate::tuning::DEFAULT_JOB_MAX_HISTORY,
                concurrency: crate::jobs::ConcurrencyConfig {
                    page_concurrency: 1,
                    per_source_download_concurrency: 1,
                    scan_concurrency: 1,
                },
                svc_cell: Arc::clone(&svc_cell),
            },
        )
        .await
        .expect("JobManager::new failed in test");

        let svc = Self {
            db: pool.clone(),
            wasm_runtime,
            sources: Arc::new(SourceRegistry::new()),
            settings: Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client,
            proxy_client,
            refresh_tx,
            refresh_task: Arc::new(tokio::sync::Mutex::new(None)),
            cache: RequestCache::new(),
            ext_cache: std::sync::Arc::new(kani_core::cache::InMemoryCache::new()),
            shutdown_token,
            tracker_registry: Arc::new(tokio::sync::RwLock::new(tracker_registry)),
            cover_retry_queue: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            email_service: Arc::new(tokio::sync::RwLock::new(None)),
            encryption: None,
            webhook_service: webhooks::WebhookService::new(pool),
            metadata_provider_registry: Arc::new(tokio::sync::RwLock::new(
                metadata_provider::MetadataProviderRegistry::new(),
            )),
            job_manager,
            install_locks: Arc::new(DashMap::new()),
            mock_sources: Arc::new(DashMap::new()),
        };
        *svc_cell.lock().expect("svc_cell lock") = Some(svc.clone());
        svc
    }

    #[cfg(any(test, feature = "test-util"))]
    pub fn register_mock_source(
        &self,
        source_id: i64,
        fetcher: Arc<dyn kani_core::downloader::PageListFetcher>,
    ) {
        self.mock_sources.insert(source_id, fetcher);
    }

    #[cfg(any(test, feature = "test-util"))]
    pub async fn scan_and_load_yaml_dir_for_test(
        &self,
        wasm_dir: &std::path::Path,
    ) -> crate::error::Result<()> {
        let pref_schemas: DashMap<i64, Vec<kani_core::PreferenceSpec>> = DashMap::new();
        Self::scan_and_register_sources(
            &self.db,
            wasm_dir,
            self.smart_client.clone(),
            &self.wasm_runtime,
            &pref_schemas,
        )
        .await?;

        use sqlx::Row as _;
        let rows =
            sqlx::query("SELECT id, name FROM sources WHERE enabled = 1 AND deleted_at IS NULL")
                .fetch_all(&self.db)
                .await?;
        let sources: Vec<(i64, String)> = rows
            .into_iter()
            .filter_map(|r| {
                let id: i64 = r.try_get("id").ok()?;
                let name: String = r.try_get("name").ok()?;
                Some((id, name))
            })
            .collect();

        for (source_id, source_name) in sources {
            let yaml_path = wasm_dir.join(format!("{source_name}.yaml"));
            if !yaml_path.exists() {
                continue;
            }
            let text = tokio::fs::read_to_string(&yaml_path)
                .await
                .map_err(|e| crate::error::ServiceError::Core(kani_core::Error::Io(e)))?;
            match kani_yaml::parse_and_validate(&text, &yaml_path) {
                Ok(ext) => {
                    let prefs = Self::load_pref_map_static(&self.db, source_id).await?;
                    let backend = crate::source::loader::build_yaml_source(
                        std::sync::Arc::new(ext),
                        self.smart_client.clone(),
                        std::sync::Arc::clone(&self.ext_cache),
                        format!("{source_name}:"),
                        prefs,
                    );
                    self.sources.insert(source_id, backend);
                }
                Err(errs) => {
                    let msg = errs
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; ");
                    tracing::warn!("YAML source '{source_name}' skipped in test: {msg}");
                }
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-util"))]
    pub async fn load_yaml_sources_from_dir_for_test(
        &self,
        wasm_dir: &std::path::Path,
    ) -> crate::error::Result<()> {
        use sqlx::Row as _;
        let rows =
            sqlx::query("SELECT id, name FROM sources WHERE enabled = 1 AND deleted_at IS NULL")
                .fetch_all(&self.db)
                .await?;

        for row in rows {
            let source_id: i64 = row.try_get("id").unwrap_or_default();
            let source_name: String = row.try_get("name").unwrap_or_default();
            let yaml_path = wasm_dir.join(format!("{source_name}.yaml"));
            if !yaml_path.exists() {
                continue;
            }
            let text = tokio::fs::read_to_string(&yaml_path)
                .await
                .map_err(|e| crate::error::ServiceError::Core(kani_core::Error::Io(e)))?;
            match kani_yaml::parse_and_validate(&text, &yaml_path) {
                Ok(ext) => {
                    let prefs = Self::load_pref_map_static(&self.db, source_id).await?;
                    let backend = crate::source::loader::build_yaml_source(
                        std::sync::Arc::new(ext),
                        self.smart_client.clone(),
                        std::sync::Arc::clone(&self.ext_cache),
                        format!("{source_name}:"),
                        prefs,
                    );
                    self.sources.insert(source_id, backend);
                }
                Err(errs) => {
                    let msg = errs
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join("; ");
                    tracing::warn!("YAML source '{source_name}' skipped in test: {msg}");
                }
            }
        }
        Ok(())
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
    ///
    /// Cancellation audit: every `select!` in the background loops pairs
    /// `shutdown_token.cancelled()` against a `sleep`/`interval.tick()` only —
    /// both are cancel-safe and hold no DB transaction across the await, so a
    /// shutdown can never abort an in-flight write mid-transaction.
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
                    match state
                        .scan_for_new_chapters(crate::ids::MangaId(manga_db_id))
                        .await
                    {
                        Ok(new_ids) if !new_ids.is_empty() => {
                            tracing::info!(
                                "Found {} new chapters for manga {}",
                                new_ids.len(),
                                manga_db_id
                            );
                            if effective_auto_download {
                                let filtered_ids = state
                                    .filter_chapters_by_rules(
                                        crate::ids::MangaId(manga_db_id),
                                        new_ids
                                            .iter()
                                            .copied()
                                            .map(crate::ids::ChapterId)
                                            .collect(),
                                    )
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

                                    let mut join_set = tokio::task::JoinSet::new();
                                    for new_id in filtered_ids {
                                        let s = state.clone();
                                        join_set.spawn(async move {
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
                                        });
                                    }
                                    while let Some(result) = join_set.join_next().await {
                                        if let Err(e) = result {
                                            tracing::error!("Chapter enqueue task panicked: {e}");
                                        }
                                    }
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
    pub async fn schedule_cover_retry(&self, manga_id: MangaId) {
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

                let ids: Vec<MangaId> = state.cover_retry_queue.lock().await.drain().collect();
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

    pub fn spawn_cache_prune(&self) {
        let cache = std::sync::Arc::clone(&self.ext_cache);
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = interval.tick() => {}
                }
                cache.prune_expired().await;
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
        user_id: Option<crate::ids::UserId>,
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

pub(crate) async fn cleanup_staging_dirs(
    library_path: &std::path::Path,
    pool: &sqlx::sqlite::SqlitePool,
) -> std::io::Result<()> {
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

            let suffix = &name[".tmp_staging_".len()..];
            if let Ok(chapter_id) = suffix.parse::<i64>() {
                let resume_offset: Option<i64> = sqlx::query_scalar!(
                    "SELECT resume_offset FROM chapters WHERE id = ?",
                    chapter_id
                )
                .fetch_optional(pool)
                .await
                .unwrap_or(None);

                if resume_offset.is_some_and(|o| o > 0) {
                    tracing::debug!(
                        "Keeping staging dir for chapter {} (resume_offset={})",
                        chapter_id,
                        resume_offset.unwrap_or(0)
                    );
                    continue;
                }
            }

            tracing::info!("Removing orphaned staging directory: {:?}", entry.path());
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
