use std::sync::Arc;

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::DownloaderManager;
use kani_core::source_manager::SourceManager;
use kani_core::wasm::WasmRuntime;

use crate::error::AppError;
use crate::models::{Settings, SharedChapter, Source};

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
}

impl AppState {
    pub async fn new() -> Result<Self, AppError> {
        let wasm_runtime = Arc::new(WasmRuntime::new().map_err(AppError::CoreError)?);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite://kani.db?mode=rwc")
            .await?;

        sqlx::migrate!("../migrations").run(&pool).await?;

        let mut sources_map = std::collections::HashMap::new();

        let settings = sqlx::query_as!(Settings, "SELECT * FROM settings")
            .fetch_one(&pool)
            .await?;

        let flaresolverr_url = if settings.flaresolverr_url.is_empty() {
            None
        } else {
            Some(settings.flaresolverr_url.clone())
        };

        let global_smart_client = kani_core::http::SmartClient::new(flaresolverr_url)?;

        if let Err(e) = Self::scan_and_register_sources(
            &pool,
            &settings.wasm_storage_path,
            global_smart_client.clone(),
            &wasm_runtime,
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
            let bytes = tokio::fs::read(
                settings
                    .wasm_storage_path
                    .join(format!("{}.wasm", source.name)),
            )
            .await
            .map_err(AppError::IoError)?;
            let component = wasm_runtime
                .compile_component(&bytes)
                .map_err(AppError::CoreError)?;

            let source_manager = SourceManager::new(
                wasm_runtime.engine().clone(),
                component,
                wasm_runtime.linker().clone(),
                global_smart_client.clone(),
                Some(source.base_url),
                25,
                1,
            )
            .await
            .map_err(AppError::CoreError)?;

            sources_map.insert(source.id, Arc::new(source_manager));
        }

        let downloader = DownloaderManager::new(
            global_smart_client.clone(),
            settings.concurrent_page_downloads.try_into()?,
            settings.chapter_queue_size.try_into()?,
            settings.max_retries,
            settings.initial_retry_delay_ms,
        )
        .await
        .map_err(AppError::CoreError)?;

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: std::sync::Arc::new(tokio::sync::RwLock::new(sources_map)),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client: global_smart_client,
        })
    }

    pub async fn get_popular_manga(&self, id: i64, page: i32) -> Result<String, AppError> {
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
            .get_popular_manga(page)
            .await?;

        serde_json::to_string(&result).map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
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
            .get_manga_details(manga_id)
            .await?;

        let shared_result = convert_to_shared_manga_info(result);

        serde_json::to_string(&shared_result)
            .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
    }

    pub async fn get_chapter_list(&self, id: i64, manga_id: &str) -> Result<String, AppError> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
        };

        let mut all_chapters = Vec::new();

        let mut instance = source_manager.lease_instance().await?;

        let mut page = 1i32;

        loop {
            let result = instance.get_chapter_list(manga_id, page).await?;

            let has_next_page = result.has_next_page;
            all_chapters.extend(result.chapters);

            if !has_next_page {
                break;
            }

            page += 1;
        }

        let combined = kani_core::wasm::kani::extension::types::ChapterList {
            chapters: all_chapters,
            has_next_page: false,
        };

        serde_json::to_string(&combined).map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
    }

    pub async fn get_chapter_list_paged(
        &self,
        id: i64,
        manga_id: &str,
        page: i32,
    ) -> Result<String, AppError> {
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
            .get_chapter_list(manga_id, page)
            .await?;

        serde_json::to_string(&result).map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))
    }

    pub async fn start_download(&self, chapter_id: i64) -> Result<(), AppError> {
        let record = sqlx::query!("SELECT c.source_chapter_id, c.name, c.chapter_number, c.volume, c.language, c.download_status, m.source_id, m.source_manga_id, m.name as manga_name, m.description, s.base_url FROM chapters c join manga m on c.manga_id = m.id join sources s on m.source_id = s.id WHERE c.id = ?", chapter_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Chapter {chapter_id} not found")))?;

        if record.download_status > 0 {
            return Err(AppError::InternalServerError(format!("Chapter {} is already downloaded or in progress.", chapter_id)));
        }

        let authors_record = sqlx::query!("SELECT GROUP_CONCAT(p.name, ', ') as names FROM manga_authors ma JOIN people p ON ma.person_id = p.id WHERE ma.manga_id = (SELECT manga_id FROM chapters WHERE id = ?)", chapter_id)
            .fetch_one(&self.db)
            .await?;
        let authors = authors_record.names;

        let artists_record = sqlx::query!("SELECT GROUP_CONCAT(p.name, ', ') as names FROM manga_artists ma JOIN people p ON ma.person_id = p.id WHERE ma.manga_id = (SELECT manga_id FROM chapters WHERE id = ?)", chapter_id)
            .fetch_one(&self.db)
            .await?;
        let artists = artists_record.names;

        let tags_record = sqlx::query!("SELECT GROUP_CONCAT(t.name, ', ') as names FROM manga_tags mt JOIN tags t ON mt.tag_id = t.id WHERE mt.manga_id = (SELECT manga_id FROM chapters WHERE id = ?)", chapter_id)
            .fetch_one(&self.db)
            .await?;
        let tags = tags_record.names;

        let chapter = {
            let source_manager = {
                let sources = self.sources.read().await;

                sources.get(&record.source_id).cloned().ok_or_else(|| {
                    AppError::NotFound(format!("Source {} not found", record.source_id))
                })?
            };

            let chapter_generated = source_manager
                .lease_instance()
                .await?
                .get_pages(&record.source_manga_id, &record.source_chapter_id)
                .await?;

            let json = serde_json::to_value(&chapter_generated)
                .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))?;
            serde_json::from_value::<SharedChapter>(json)
                .map_err(|e| AppError::CoreError(kani_core::Error::Json(e)))?
        };

        let library_path = self.settings.read().await.library_path.clone();

        let name = chapter_name(record.volume, record.chapter_number);

        let safe_manga_name = kani_core::sanitize::sanitize_filename(&record.manga_name);
        let path = library_path.join(safe_manga_name);

        let web_url = format!("{}/{}", record.base_url, record.source_manga_id);

        let comic_info = kani_core::comic_info::ComicInfo {
            xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance",
            series: record.manga_name,
            title: record.name,
            number: record.chapter_number,
            volume: record.volume,
            summary: record.description,
            language_iso: Some(record.language),
            writer: authors,
            penciller: artists,
            genre: tags,
            web: Some(web_url),
        };

        let xml_payload =
            kani_core::comic_info::build_xml(&comic_info).map_err(AppError::CoreError)?;

        let _ = sqlx::query!("UPDATE chapters SET download_status = 1 WHERE id = ?", chapter_id)
            .execute(&self.db)
            .await;

        self.downloader
            .queue_chapter(chapter_id, chapter, name, path, Some(xml_payload))
            .await
            .map_err(AppError::CoreError)?;

        Ok(())
    }

    pub async fn delete_downloaded(&self, chapter_id: i64) -> Result<(), AppError> {
        let record = sqlx::query!("SELECT c.download_status, c.volume, c.chapter_number, m.name as manga_name FROM chapters c join manga m on c.manga_id = m.id WHERE c.id = ?", chapter_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Chapter {chapter_id} not found")))?;

        if record.download_status != 2 {
            return Err(AppError::InternalServerError(format!("Chapter {chapter_id} is not downloaded.")));
        }

        let name = chapter_name(record.volume, record.chapter_number);

        let library_path = self.settings.read().await.library_path.clone();
        let safe_manga_name = kani_core::sanitize::sanitize_filename(&record.manga_name);
        let path = library_path.join(safe_manga_name);
        let cbz_path = path.join(format!("{}.cbz", &name));

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

    async fn scan_and_register_sources(
        db: &SqlitePool,
        wasm_storage_path: &std::path::Path,
        smart_client: kani_core::http::SmartClient,
        wasm_runtime: &WasmRuntime,
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

                    let metadata = {
                        let mut inst =
                            kani_core::sources::SourceInstance::new(smart_client.clone(), None);
                        inst.load(wasm_runtime.engine(), &component, wasm_runtime.linker())
                            .await
                            .map_err(AppError::CoreError)?;
                        inst.get_metadata().await.map_err(AppError::CoreError)?
                    };

                    match serde_json::to_value(&metadata)
                        .and_then(serde_json::from_value::<kani_shared::ExtensionMetadata>)
                    {
                        Ok(metadata) => {
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
}

fn convert_to_shared_manga_info(
    info: kani_core::wasm::kani::extension::types::MangaInfo,
) -> kani_shared::MangaInfo {
    use kani_core::wasm::kani::extension::types::MangaStatus as CoreMangaStatus;
    use kani_shared::MangaStatus as SharedMangaStatus;

    let status = match info.status {
        CoreMangaStatus::Ongoing => SharedMangaStatus::Ongoing,
        CoreMangaStatus::Completed => SharedMangaStatus::Completed,
        CoreMangaStatus::Hiatus => SharedMangaStatus::Hiatus,
        CoreMangaStatus::Cancelled => SharedMangaStatus::Cancelled,
        CoreMangaStatus::Unknown => SharedMangaStatus::Unknown,
    };

    kani_shared::MangaInfo {
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

fn chapter_name(volume: Option<i64>, chapter_number: f64) -> String {
    volume.map_or_else(
        || format!("Ch. {}", chapter_number),
        |volume| format!("Vol. {volume} - Ch. {}", chapter_number),
    )
}