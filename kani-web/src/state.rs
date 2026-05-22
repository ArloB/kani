use crate::error::AppError;
use crate::logging::LogHandle;
use bytes::Bytes;
use kani_app::AppService;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// Web-layer application state.
///
/// Business logic lives in [`AppService`] (kani-app). This struct adds the
/// HTTP-specific extras — proxy cryptography, per-host rate-limit semaphores,
/// a boot-time identifier used by the SSE reconnect protocol, and the
/// in-memory log ring buffer handle.
#[derive(Clone)]
pub struct AppState {
    pub service: Arc<AppService>,
    pub proxy_secret: Arc<[u8; 32]>,
    pub proxy_semaphores: moka::future::Cache<String, Arc<tokio::sync::Semaphore>>,
    pub proxy_throttle: moka::future::Cache<String, Arc<tokio::sync::Mutex<std::time::Instant>>>,
    pub proxy_coalesce: moka::future::Cache<String, Arc<(Bytes, String)>>,
    pub boot_id: String,
    /// Set to `true` by the restart handler; causes `main` to exit with code 42
    /// instead of 0, so an entrypoint wrapper can restart the process.
    pub restart_requested: Arc<AtomicBool>,
    pub log_handle: Arc<LogHandle>,
}

impl AppState {
    pub async fn new(log_handle: Arc<LogHandle>) -> Result<Self> {
        let service = Arc::new(AppService::new().await?);
        Ok(Self {
            proxy_secret: Arc::new(crate::proxy::load_or_generate_secret()),
            proxy_semaphores: moka::future::Cache::builder()
                .max_capacity(1_000)
                .time_to_idle(std::time::Duration::from_secs(3_600))
                .build(),
            proxy_throttle: moka::future::Cache::builder()
                .max_capacity(1_000)
                .time_to_idle(std::time::Duration::from_secs(3_600))
                .build(),
            proxy_coalesce: moka::future::Cache::builder()
                .max_capacity(50 * 1024 * 1024)
                .time_to_live(std::time::Duration::from_secs(30))
                .weigher(|_k, v: &Arc<(Bytes, String)>| v.0.len().min(u32::MAX as usize) as u32)
                .build(),
            boot_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
            restart_requested: Arc::new(AtomicBool::new(false)),
            log_handle,
            service,
        })
    }
}

/// Allows REST handlers to call `state.method()` without `state.service.method()`.
impl std::ops::Deref for AppState {
    type Target = AppService;
    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

pub use kani_app::chapter_name;

#[cfg(test)]
impl AppState {
    pub async fn new_for_test(pool: sqlx::SqlitePool) -> Self {
        let service = Arc::new(AppService::new_for_test(pool).await);
        let (_, log_handle) = crate::logging::RingBufferLayer::new(100);
        Self {
            service,
            proxy_secret: Arc::new([0u8; 32]),
            proxy_semaphores: moka::future::Cache::builder().max_capacity(100).build(),
            proxy_throttle: moka::future::Cache::builder().max_capacity(100).build(),
            proxy_coalesce: moka::future::Cache::builder().max_capacity(100).build(),
            boot_id: "test-boot-id".to_string(),
            restart_requested: Arc::new(AtomicBool::new(false)),
            log_handle,
        }
    }
}
