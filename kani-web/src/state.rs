use crate::error::AppError;
use kani_app::AppService;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// Web-layer application state.
///
/// Business logic lives in [`AppService`] (kani-app). This struct adds the
/// HTTP-specific extras — proxy cryptography, per-host rate-limit semaphores,
/// and a boot-time identifier used by the SSE reconnect protocol.
#[derive(Clone)]
pub struct AppState {
    pub service: Arc<AppService>,
    pub proxy_secret: Arc<[u8; 32]>,
    pub proxy_semaphores: moka::future::Cache<String, Arc<tokio::sync::Semaphore>>,
    pub boot_id: String,
    /// Set to `true` by the restart handler; causes `main` to exit with code 42
    /// instead of 0, so an entrypoint wrapper can restart the process.
    pub restart_requested: Arc<AtomicBool>,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let service = Arc::new(AppService::new().await?);
        Ok(Self {
            proxy_secret: Arc::new(crate::proxy::load_or_generate_secret()),
            proxy_semaphores: moka::future::Cache::builder()
                .max_capacity(1_000)
                .time_to_idle(std::time::Duration::from_secs(3_600))
                .build(),
            boot_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .to_string(),
            restart_requested: Arc::new(AtomicBool::new(false)),
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

// Re-export for main.rs which uses kani_web::state::chapter_name
pub use kani_app::chapter_name;
