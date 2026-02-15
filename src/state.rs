use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::DownloaderManager;
use kani_core::sources::SourceHost;
use kani_core::wasm::WasmRuntime;
use kani_core::wasmtime::{self, Val};

use crate::error::AppError;
use crate::models::{Chapter, Settings, Source};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub sources: std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<i64, SourceHost>>>,
    pub settings: std::sync::Arc<tokio::sync::RwLock<Settings>>,
    pub downloader: DownloaderManager,
    pub http_client: rquest::Client,
}

impl AppState {
    pub async fn new() -> Result<Self, AppError> {
        let wasm_runtime = Arc::new(WasmRuntime::new().map_err(AppError::CoreError)?);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite://kani.db?mode=rwc")
            .await?;

        sqlx::migrate!().run(&pool).await?;

        let mut sources_map = std::collections::HashMap::new();

        let settings = sqlx::query_as!(Settings, "SELECT * FROM settings")
            .fetch_one(&pool)
            .await?;

        // Scan for new WASM sources
        let flaresolverr_url = if settings.flaresolverr_url.is_empty() {
            None
        } else {
            Some(settings.flaresolverr_url.clone())
        };

        if let Err(e) = Self::scan_and_register_sources(
            &pool,
            &settings.wasm_storage_path,
            flaresolverr_url.clone(),
            wasm_runtime.engine(),
            wasm_runtime.linker(),
        )
        .await
        {
            tracing::error!("Failed to scan and register sources: {}", e);
        }

        let sources = sqlx::query_as!(
            Source,
            "SELECT id, name, version, base_url FROM sources WHERE enabled = 1"
        )
        .fetch_all(&pool)
        .await?;

        for source in sources {
            let solver_url = if settings.flaresolverr_url.is_empty() {
                None
            } else {
                Some(settings.flaresolverr_url.clone())
            };

            let source_host = SourceHost::new(solver_url, &source.name)
                .load(wasm_runtime.engine(), &settings.wasm_storage_path)
                .await?;

            sources_map.insert(source.id, source_host);
        }

        let downloader = DownloaderManager::new(
            &settings.flaresolverr_url,
            settings.concurrent_page_downloads.try_into()?,
            settings.chapter_queue_size.try_into()?,
            settings.max_retries,
            settings.initial_retry_delay_ms,
        )
        .map_err(AppError::CoreError)?;

        let http_client = rquest::Client::builder()
            .emulation(rquest_util::Emulation::Chrome126)
            .build()?;

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: std::sync::Arc::new(tokio::sync::Mutex::new(sources_map)),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            http_client,
        })
    }

    pub async fn get_popular_manga(&self, id: i64, page: i32) -> Result<String, AppError> {
        let linker = self.wasm_runtime.linker();

        self.sources
            .lock()
            .await
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
            .call_function_json(linker, "get_popular_manga", vec![Val::I32(page)])
            .await
            .map(|v: serde_json::Value| v.to_string())
            .map_err(AppError::CoreError)
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn search_manga(&self, id: i64, query: &str, page: i32) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

        let linker = self.wasm_runtime.linker();

        // Allocate string in WASM memory
        let (query_ptr, query_len) = source
            .write_string(linker, query)
            .await
            .map_err(AppError::CoreError)?;

        let result: Result<serde_json::Value, _> = source
            .call_function_json(
                linker,
                "search_manga",
                vec![Val::I32(query_ptr), Val::I32(query_len), Val::I32(page)],
            )
            .await;

        let return_val = result.map(|v| v.to_string()).map_err(AppError::CoreError);

        if let Err(e) = source.deallocate_memory(query_ptr, query_len).await {
            tracing::error!("Failed to deallocate query string in WASM: {}", e);
        }

        return_val
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn get_manga_details(&self, id: i64, manga_id: &str) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

        let linker = self.wasm_runtime.linker();

        let (manga_id_ptr, manga_id_len) = source
            .write_string(linker, manga_id)
            .await
            .map_err(AppError::CoreError)?;

        let result: Result<serde_json::Value, _> = source
            .call_function_json(
                linker,
                "get_manga_details",
                vec![Val::I32(manga_id_ptr), Val::I32(manga_id_len)],
            )
            .await;

        let return_val = result.map(|v| v.to_string()).map_err(AppError::CoreError);

        if let Err(e) = source.deallocate_memory(manga_id_ptr, manga_id_len).await {
            tracing::error!("Failed to deallocate query string in WASM: {}", e);
        }

        return_val
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn get_chapter_list(
        &self,
        id: i64,
        manga_id: &str,
        page: i32,
    ) -> Result<String, AppError> {
        let mut sources = self.sources.lock().await;

        let source = sources
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

        let linker = self.wasm_runtime.linker();

        let (manga_id_ptr, manga_id_len) = source
            .write_string(linker, manga_id)
            .await
            .map_err(AppError::CoreError)?;

        let result: Result<serde_json::Value, _> = source
            .call_function_json(
                linker,
                "get_chapter_list",
                vec![
                    Val::I32(manga_id_ptr),
                    Val::I32(manga_id_len),
                    Val::I32(page),
                ],
            )
            .await;

        let return_val = result.map(|v| v.to_string()).map_err(AppError::CoreError);

        if let Err(e) = source.deallocate_memory(manga_id_ptr, manga_id_len).await {
            tracing::error!("Failed to deallocate query string in WASM: {}", e);
        }

        return_val
    }

    #[allow(clippy::significant_drop_tightening)]
    pub async fn start_download(
        &self,
        id: i64,
        manga_id: &str,
        chapter_id: &str,
    ) -> Result<(), AppError> {
        let chapter = {
            let mut sources = self.sources.lock().await;

            let source = sources
                .get_mut(&id)
                .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

            let linker = self.wasm_runtime.linker();

            let (manga_id_ptr, manga_id_len) = source
                .write_string(linker, manga_id)
                .await
                .map_err(AppError::CoreError)?;

            let (chapter_id_ptr, chapter_id_len) = source
                .write_string(linker, chapter_id)
                .await
                .map_err(AppError::CoreError)?;

            let chapter_res: Result<Chapter, _> = source
                .call_function_json(
                    linker,
                    "get_pages",
                    vec![
                        Val::I32(manga_id_ptr),
                        Val::I32(manga_id_len),
                        Val::I32(chapter_id_ptr),
                        Val::I32(chapter_id_len),
                    ],
                )
                .await;

            if let Err(e) = source.deallocate_memory(manga_id_ptr, manga_id_len).await {
                tracing::error!("Failed to deallocate manga_id string in WASM: {}", e);
            }

            if let Err(e) = source
                .deallocate_memory(chapter_id_ptr, chapter_id_len)
                .await
            {
                tracing::error!("Failed to deallocate chapter_id string in WASM: {}", e);
            }

            chapter_res.map_err(AppError::CoreError)?
        };

        let library_path = self.settings.read().await.library_path.clone();

        self.downloader
            .queue_chapter(
                chapter,
                library_path.join(/* Library system to be developed */ "test"),
            )
            .await
            .map_err(AppError::CoreError)?;

        Ok(())
    }

    pub async fn get_metadata(&self, id: i64) -> Result<kani_shared::ExtensionMetadata, AppError> {
        let linker = self.wasm_runtime.linker();
        let json = self
            .sources
            .lock()
            .await
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
            .call_function_str(linker, "get_metadata", vec![])
            .await?;

        let metadata: kani_shared::ExtensionMetadata = serde_json::from_str(&json)
            .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))?;

        Ok(metadata)
    }

    async fn scan_and_register_sources(
        db: &SqlitePool,
        wasm_storage_path: &std::path::Path,
        flaresolverr_url: Option<String>,
        engine: &wasmtime::Engine,
        linker: &wasmtime::Linker<kani_core::wasm::HostState>,
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

                // Check if source exists in DB
                let exists = sqlx::query!("SELECT id FROM sources WHERE name = ?", filename)
                    .fetch_optional(db)
                    .await?
                    .is_some();

                if !exists {
                    match SourceHost::new(flaresolverr_url.clone(), filename)
                        .load(engine, wasm_storage_path)
                        .await
                    {
                        Ok(mut source_host) => {
                            let json = match source_host
                                .call_function_str(linker, "get_metadata", vec![])
                                .await
                            {
                                Ok(j) => j,
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to get metadata for {}: {}",
                                        filename,
                                        e
                                    );
                                    continue;
                                }
                            };

                            let metadata: kani_shared::ExtensionMetadata =
                                match serde_json::from_str(&json) {
                                    Ok(m) => m,
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to parse metadata for {}: {}",
                                            filename,
                                            e
                                        );
                                        continue;
                                    }
                                };

                            // Insert into DB
                            if let Err(e) = sqlx::query!(
                                "INSERT INTO sources (name, version, base_url, enabled) VALUES (?, ?, ?, 1)",
                                filename,
                                metadata.version,
                                metadata.base_url
                            )
                            .execute(db)
                            .await
                            {
                                tracing::error!("Failed to register source {}: {}", filename, e);
                            } else {
                                tracing::info!("Registered new source: {}", filename);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to load new source {}: {}", filename, e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
