use std::path::PathBuf;
use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::DownloaderManager;
use kani_core::sources::SourceHost;
use kani_core::wasm::WasmRuntime;
use kani_core::wasmtime::Val;

use crate::error::AppError;
use crate::models::{Chapter, Settings, Source};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub sources: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<i64, SourceHost>>>,
    pub settings: std::sync::Arc<tokio::sync::RwLock<Settings>>,
    pub downloader: DownloaderManager,
}

impl AppState {
    pub async fn new() -> Result<Self, AppError> {
        let wasm_runtime = Arc::new(WasmRuntime::new().map_err(|e| AppError::CoreError(e))?);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite://kani.db?mode=rwc")
            .await?;

        sqlx::migrate!().run(&pool).await?;

        let mut sources_map = std::collections::HashMap::new();

        let sources = sqlx::query_as!(
            Source,
            "SELECT id, name, version FROM sources WHERE enabled = 1"
        )
        .fetch_all(&pool)
        .await?;

        let settings = sqlx::query_as!(Settings, "SELECT * FROM settings")
            .fetch_one(&pool)
            .await?;

        for source in sources {
            let wasm_path = format!("{}/{}.wasm", settings.wasm_storage_path, source.id);
            tracing::info!("Loading source: {} ({})", source.name, wasm_path);
            match tokio::fs::read(&wasm_path).await {
                Ok(bytes) => match SourceHost::new(wasm_runtime.engine(), &bytes) {
                    Ok(host) => {
                        sources_map.insert(source.id, host);
                        tracing::info!("Successfully loaded source: {}", source.name);
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to compile module for source {}: {}",
                            source.name,
                            e
                        );
                    }
                },
                Err(e) => {
                    tracing::error!("Failed to read WASM file for source {}: {}", source.name, e);
                }
            }
        }

        let downloader = DownloaderManager::new(
            &settings.flaresolverr_url,
            settings.concurrent_page_downloads.try_into()?,
            settings.chapter_queue_size.try_into()?,
            settings.max_retries,
            settings.initial_retry_delay_ms,
        )
        .map_err(|e| AppError::CoreError(e))?;

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: std::sync::Arc::new(tokio::sync::Mutex::new(sources_map)),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
        })
    }

    pub async fn get_popular_manga(&self, id: i64, page: i32) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?;

        let linker = self.wasm_runtime.linker();
        source
            .call_function_str(linker, "get_popular_manga", vec![Val::I32(page)])
            .await
            .map_err(|e| AppError::CoreError(e))
    }

    pub async fn search_manga(&self, id: i64, query: &str, page: i32) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?;

        let linker = self.wasm_runtime.linker();
        source
            .call_function_str(
                linker,
                "search_manga",
                vec![
                    Val::I32(page),
                    Val::I32(query.len() as i32),
                    Val::I32(query.as_ptr() as i32),
                ],
            )
            .await
            .map_err(|e| AppError::CoreError(e))
    }

    pub async fn get_manga_details(&self, id: i64, manga_id: i32) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?;

        let linker = self.wasm_runtime.linker();
        source
            .call_function_str(linker, "get_manga_details", vec![Val::I32(manga_id)])
            .await
            .map_err(|e| AppError::CoreError(e))
    }

    pub async fn start_download(
        &self,
        id: i64,
        manga_id: i32,
        chapter_id: i32,
    ) -> Result<(), AppError> {
        let chapter = {
            let mut sources = self.sources.lock().await;

            let source = sources
                .get_mut(&id)
                .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?;

            let linker = self.wasm_runtime.linker();
            let pages = source
                .call_function_str(
                    linker,
                    "get_pages",
                    vec![Val::I32(manga_id), Val::I32(chapter_id)],
                )
                .await
                .map_err(|e| AppError::CoreError(e))?;

            serde_json::from_str::<Chapter>(&pages)?
        };

        self.downloader
            .queue_chapter(chapter, /* chapter file path */ PathBuf::from(""))
            .await
            .map_err(|e| AppError::CoreError(e))?;

        Ok(())
    }
}
