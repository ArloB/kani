use std::path;
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

use kani_core::downloader::{DownloadTask, DownloaderManager};
use kani_core::source_manager::SourceManager;
use kani_core::wasm::WasmRuntime;

use crate::error::AppError;
use crate::models::{Settings, Source};

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

        let settings = sqlx::query_as!(Settings, "SELECT flaresolverr_url, library_path, wasm_storage_path, concurrent_page_downloads, chapter_queue_size, max_retries, initial_retry_delay_ms, max_wasm_instances FROM settings")
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
        tracing::info!("Sources scanned and registered");

        let sources = sqlx::query_as!(
            Source,
            "SELECT id, name, version, base_url FROM sources WHERE enabled = 1"
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
        tracing::info!("Sources loaded");

        let downloader = DownloaderManager::new(
            global_smart_client.clone(),
            settings.concurrent_page_downloads.try_into()?,
            settings.chapter_queue_size.try_into()?,
            settings.max_retries,
            settings.initial_retry_delay_ms,
            10000,
        )
        .await
        .map_err(AppError::CoreError)?;
        tracing::info!("Downloader manager created");

        let proxy_client = kani_core::http::SmartClient::new_proxy()?;
        tracing::info!("Proxy client created");

        Ok(Self {
            db: pool,
            wasm_runtime,
            sources: std::sync::Arc::new(tokio::sync::RwLock::new(sources_map)),
            settings: std::sync::Arc::new(tokio::sync::RwLock::new(settings)),
            downloader,
            smart_client: global_smart_client,
            proxy_client,
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

    pub async fn save_to_library(
        &self,
        source_id: i64,
        manga_id: &str,
    ) -> Result<i64, AppError> {
        let manga: kani_shared::MangaInfo = serde_json::from_str(
            &self.get_manga_details(source_id, manga_id).await?,
        )
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse manga: {}", e)))?;

        let mut tx = self.db.begin().await?;
        let status: i64 = manga.status.into();

        let insert_result = sqlx::query!(
            "INSERT OR IGNORE INTO manga (source_manga_id, source_id, name, cover_url, description, status) \
             VALUES (?, ?, ?, ?, ?, ?)",
            manga.id,
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
            manga.id,
            source_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if we_inserted {
            for author in &manga.authors {
                sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", author)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query!(
                    "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                    SELECT ?, 'author', id FROM people WHERE name = ?",
                    manga_row_id,
                    author
                )
                .execute(&mut *tx)
                .await?;
            }

            for artist in &manga.artists {
                sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", artist)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query!(
                    "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                    SELECT ?, 'artist', id FROM people WHERE name = ?",
                    manga_row_id,
                    artist
                )
                .execute(&mut *tx)
                .await?;
            }

            for tag in &manga.tags {
                sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", tag)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query!(
                    "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
                    SELECT ?, id FROM tags WHERE name = ?",
                    manga_row_id,
                    tag
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;

        if we_inserted {
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
        let res = self.get_chapter_list_paged(source_id, manga_id, page).await?;
        let chapter_list: kani_shared::ChapterList = serde_json::from_str(&res)
            .map_err(|e| AppError::InternalServerError(format!("Failed to parse chapter list: {}", e)))?;

        if chapter_list.chapters.is_empty() {
            return Ok(false);
        }

        for chunk in chapter_list.chapters.chunks(100) {
            let mut query_builder = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO chapters (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at) "
            );

            query_builder.push_values(chunk, |mut b, chapter| {
                b.push_bind(manga_row_id)
                 .push_bind(chapter.id.clone())
                 .push_bind(chapter.title.clone())
                 .push_bind(chapter.number)
                 .push_bind(chapter.language.clone())
                 .push_bind(chapter.volume)
                 .push_bind(chapter.scanlator.clone())
                 .push_bind(chapter.date_uploaded);
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

    // Download chapter(s)
    async fn build_download_task(&self, chapter_id: i64) -> Result<DownloadTask, AppError> {
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

    async fn enqueue_claimed_chapter(&self, chapter_id: i64) -> Result<(), AppError> {
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