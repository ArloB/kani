//! Download manager for queuing and processing chapter downloads.

use crate::error::{self, Result};
use crate::http::{SmartClient, SmartResponse};
use crate::utilities::{assert_within_root, sanitize_filename};
use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::stream::{self, StreamExt};
use kani_shared::DownloadProgressEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, broadcast};
use tokio_util::compat::{FuturesAsyncWriteCompatExt, TokioAsyncWriteCompatExt};

/// Channel capacity for download progress events.
const PROGRESS_CHANNEL_CAPACITY: usize = 512;

/// Grace period to keep terminal states in the active map so reconnecting
/// clients can see the final status before it is cleaned up.
const TERMINAL_STATE_TTL_SECS: u64 = 30;

/// Unique identifier for a queued chapter
pub type QueueId = u64;

/// Information about a queued chapter (returned by list_queue)
#[derive(Debug, Clone)]
pub struct QueuedChapter {
    pub id: QueueId,
    pub chapter_name: String,
    pub page_count: usize,
    pub save_path: PathBuf,
}

pub struct DownloadTask {
    pub chapter_id: i64,
    pub manga_id: i64,
    pub manga_title: String,
    pub source_manager: Arc<crate::source_manager::SourceManager>,
    pub source_manga_id: String,
    pub source_chapter_id: String,
    pub name: String,
    pub library_path: PathBuf,
    pub save_path: PathBuf,
    pub comic_info: Option<crate::comic_info::ComicInfo>,
}

struct QueuedDownloadTask {
    id: QueueId,
    task: DownloadTask,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// Shared queue state accessible from both the manager and worker
struct QueueState {
    queue: VecDeque<QueuedDownloadTask>,
    active_tasks: HashMap<i64, tokio::sync::oneshot::Sender<()>>,
    next_id: AtomicU64,
}

impl QueueState {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            active_tasks: HashMap::new(),
            next_id: AtomicU64::new(1),
        }
    }

    fn generate_id(&self) -> QueueId {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ActiveDownloadState {
    pub chapter_id: i64,
    pub chapter_name: String,
    pub manga_id: i64,
    pub manga_title: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub status: ActiveDownloadStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ActiveDownloadStatus {
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Clone)]
pub struct DownloaderManager {
    queue: Arc<Mutex<QueueState>>,
    progress_tx: broadcast::Sender<DownloadProgressEvent>,
    capacity_semaphore: Arc<tokio::sync::Semaphore>,
    active: Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
    queue_semaphore: Arc<tokio::sync::Semaphore>,
}

impl DownloaderManager {
    pub fn subscribe(&self) -> broadcast::Receiver<DownloadProgressEvent> {
        self.progress_tx.subscribe()
    }
}

impl DownloaderManager {
    pub async fn new(
        smart_client: SmartClient,
        concurrent_page_downloads: usize,
        concurrent_chapters: usize,
        max_retries: i64,
        initial_retry_delay_ms: i64,
        queue_limit: usize,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(QueueState::new()));
        let (progress_tx, _) = broadcast::channel(PROGRESS_CHANNEL_CAPACITY);
        let active = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let capacity_semaphore = Arc::new(tokio::sync::Semaphore::new(queue_limit));
        let queue_semaphore = Arc::new(tokio::sync::Semaphore::new(0)); // starts empty
        let chapter_limiter = Arc::new(tokio::sync::Semaphore::new(concurrent_chapters));

        tokio::spawn(Self::run_worker(
            smart_client,
            queue.clone(),
            queue_semaphore.clone(),
            chapter_limiter,
            progress_tx.clone(),
            active.clone(),
            concurrent_page_downloads,
            max_retries,
            initial_retry_delay_ms,
        ));

        Ok(Self {
            queue,
            progress_tx,
            capacity_semaphore,
            queue_semaphore,
            active,
        })
    }

    /// Queue a chapter for download, returns the queue ID
    pub async fn queue_chapter(&self, task: DownloadTask) -> Result<QueueId> {
        let permit = self
            .capacity_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        let id = {
            let mut state = self.queue.lock().await;
            let id = state.generate_id();
            state.queue.push_back(QueuedDownloadTask {
                id,
                task,
                _permit: permit,
            });
            id
        };

        self.queue_semaphore.add_permits(1);

        Ok(id)
    }

    /// List all chapters currently in the queue (not including the one being downloaded)
    pub async fn list_queue(&self) -> Vec<QueuedChapter> {
        let state = self.queue.lock().await;
        state
            .queue
            .iter()
            .map(|queued| QueuedChapter {
                id: queued.id,
                chapter_name: queued.task.name.clone(),
                page_count: 0,
                save_path: queued.task.save_path.clone(),
            })
            .collect()
    }

    /// Remove a chapter from the queue by its ID
    /// Returns true if the chapter was found and removed, false otherwise
    pub async fn remove_from_queue(&self, id: QueueId) -> bool {
        let mut state = self.queue.lock().await;
        if let Some(pos) = state.queue.iter().position(|queued| queued.id == id) {
            state.queue.remove(pos);
            true
        } else {
            false
        }
    }

    /// Cancel a chapter download by its `chapter_id` (database ID).
    /// Returns true if it was removed from the queue or if an active download was aborted.
    pub async fn cancel_download(&self, chapter_id: i64) -> bool {
        {
            let active = self.active.read().await;
            if let Some(state) = active.get(&chapter_id) {
                match state.status {
                    ActiveDownloadStatus::Completed
                    | ActiveDownloadStatus::Failed(_)
                    | ActiveDownloadStatus::Cancelled => return false,
                    ActiveDownloadStatus::InProgress => {}
                }
            }
        }

        let mut state = self.queue.lock().await;

        // Remove from pending queue if it's there
        if let Some(pos) = state
            .queue
            .iter()
            .position(|queued| queued.task.chapter_id == chapter_id)
        {
            state.queue.remove(pos);
            return true;
        }

        // If it's currently active, send the cancel signal
        if let Some(tx) = state.active_tasks.remove(&chapter_id) {
            let _ = tx.send(());
            return true;
        }

        false
    }

    /// Get the number of chapters currently in the queue
    pub async fn queue_len(&self) -> usize {
        self.queue.lock().await.queue.len()
    }

    pub async fn snapshot(&self) -> Vec<ActiveDownloadState> {
        self.active.read().await.values().cloned().collect()
    }

    // ============================================================
    // Internal helpers
    // ============================================================

    /// Send an event on the progress broadcast channel.
    fn send_event(tx: &broadcast::Sender<DownloadProgressEvent>, event: DownloadProgressEvent) {
        let _ = tx.send(event);
    }

    async fn create_cbz(
        cbz_path: &std::path::Path,
        staged_pages: Vec<(PathBuf, String)>,
        comic_info: Option<crate::comic_info::ComicInfo>,
    ) -> Result<()> {
        let cbz_file = tokio::fs::File::create(cbz_path).await?;
        let mut zip_writer = ZipFileWriter::new(cbz_file.compat_write());

        if let Some(info) = comic_info {
            let xml_content = crate::comic_info::build_xml(&info)?;
            let comic_info_builder =
                ZipEntryBuilder::new("ComicInfo.xml".into(), Compression::Stored);
            zip_writer
                .write_entry_whole(comic_info_builder, xml_content.as_bytes())
                .await
                .map_err(|e| crate::error::Error::Other(format!("Zip error: {}", e)))?;
        }

        for (tmp_path, filename) in staged_pages {
            let mut tmp_file = tokio::fs::File::open(&tmp_path).await?;
            let entry_builder = ZipEntryBuilder::new(filename.into(), Compression::Stored);
            let mut entry_writer = zip_writer
                .write_entry_stream(entry_builder)
                .await
                .map_err(|e| crate::error::Error::Other(format!("Zip error: {}", e)))?;

            tokio::io::copy(&mut tmp_file, &mut (&mut entry_writer).compat_write()).await?;

            entry_writer
                .close()
                .await
                .map_err(|e| crate::error::Error::Other(format!("Zip error: {}", e)))?;
        }

        // close() writes the central directory + EOCD into tokio::fs::File's write
        // buffer, but does not call flush() on the underlying writer. tokio::fs::File
        // Drop schedules the OS close on a thread pool without blocking, so those bytes
        // can be lost before the caller reads the file back. Flush explicitly here.
        let mut file = zip_writer
            .close()
            .await
            .map_err(|e| crate::error::Error::Other(format!("Zip error: {}", e)))?
            .into_inner();
        file.flush().await?;

        Ok(())
    }

    async fn download_page_with_retry(
        client: &SmartClient,
        url: &str,
        page_index: i32,
        staging_dir: &std::path::Path,
        max_retries: i64,
        initial_retry_delay_ms: i64,
        base_url: &str,
    ) -> Result<(PathBuf, String)> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(initial_retry_delay_ms.try_into()?);

        loop {
            match Self::download_page(client, url, page_index, staging_dir, base_url).await {
                Ok(data) => return Ok(data),
                Err(e) => {
                    attempts += 1;
                    if attempts >= max_retries {
                        tracing::error!(
                            "Failed to download page {} after {} attempts: {}",
                            page_index,
                            max_retries,
                            e
                        );
                        return Err(e);
                    }

                    tracing::warn!(
                        "Retry {}/{} for page {} after error: {}",
                        attempts,
                        max_retries,
                        page_index,
                        e
                    );

                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }

    async fn download_page(
        client: &SmartClient,
        url: &str,
        page: i32,
        staging_dir: &std::path::Path,
        referer: &str,
    ) -> Result<(PathBuf, String)> {
        let request = client
            .inner()
            .get(url)
            .header(rquest::header::REFERER, referer)
            .build()
            .map_err(|e| crate::error::Error::Other(e.to_string()))?;

        let mut resp = client.send_request(request).await?;

        let status = resp.status();
        if !status.is_success() {
            return Err(error::Error::Other(format!(
                "HTTP {status} downloading page {page}"
            )));
        }

        let extension = Self::get_image_extension(&resp, url);

        let filename = format!("{:04}.{}", page, extension);
        let tmp_file_path = staging_dir.join(format!("{:04}.tmp", page));

        let mut file = tokio::fs::File::create(&tmp_file_path).await?;
        let mut bytes_written: u64 = 0;

        while let Some(chunk) = resp.chunk().await? {
            bytes_written += chunk.len() as u64;
            file.write_all(&chunk).await?;
        }
        file.flush().await?;

        if bytes_written == 0 {
            return Err(error::Error::Other(format!(
                "Server returned empty body for page {page}"
            )));
        }

        Ok((tmp_file_path, filename))
    }

    fn get_image_extension(resp: &SmartResponse, url: &str) -> &'static str {
        if let Some(ct) = resp
            .headers()
            .get("content-type")
            .and_then(|c| c.to_str().ok())
        {
            match ct {
                "image/jpeg" | "image/jpg" => return "jpg",
                "image/png" => return "png",
                "image/webp" => return "webp",
                "image/gif" => return "gif",
                "image/avif" => return "avif",
                _ => {}
            }
        }

        let clean_url = url.split('?').next().unwrap_or(url);

        if let Some(ext) = clean_url.rsplit('.').next() {
            match ext.to_lowercase().as_str() {
                "jpg" | "jpeg" => return "jpg",
                "png" => return "png",
                "webp" => return "webp",
                "gif" => return "gif",
                "avif" => return "avif",
                _ => {}
            }
        }

        "jpg"
    }

    async fn update_active(
        active: &Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
        chapter_id: i64,
        f: impl FnOnce(&mut ActiveDownloadState),
    ) {
        let mut map = active.write().await;
        if let Some(s) = map.get_mut(&chapter_id) {
            f(s);
        }
    }

    fn schedule_active_cleanup(
        active: Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
        chapter_id: i64,
    ) {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(TERMINAL_STATE_TTL_SECS)).await;
            active.write().await.remove(&chapter_id);
        });
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_worker(
        client_global: SmartClient,
        queue_state: Arc<Mutex<QueueState>>,
        queue_semaphore: Arc<tokio::sync::Semaphore>,
        chapter_limiter: Arc<tokio::sync::Semaphore>,
        progress_tx: broadcast::Sender<DownloadProgressEvent>,
        active: Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
        concurrent_page_downloads: usize,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) {
        loop {
            let queue_permit = match queue_semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("Download queue semaphore closed, shutting down worker");
                    break;
                }
            };
            queue_permit.forget();

            let (task, cancel_rx) = {
                let mut state = queue_state.lock().await;
                let Some(task) = state.queue.pop_front() else {
                    tracing::warn!("Download queue empty after semaphore acquire, skipping");
                    continue;
                };
                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                state.active_tasks.insert(task.task.chapter_id, cancel_tx);
                (task, cancel_rx)
            };

            let concurrency_permit = match chapter_limiter.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("Chapter limiter semaphore closed, shutting down worker");
                    break;
                }
            };

            let client = client_global.clone();
            let tx = progress_tx.clone();
            let queue_ref = queue_state.clone();
            let active_ref = active.clone();

            tokio::spawn(async move {
                let _concurrency_permit = concurrency_permit;
                let _ = Self::process_chapter(
                    task,
                    cancel_rx,
                    client,
                    tx,
                    queue_ref,
                    active_ref,
                    concurrent_page_downloads,
                    max_retries,
                    initial_retry_delay_ms,
                )
                .await;
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_chapter(
        task: QueuedDownloadTask,
        mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
        client: SmartClient,
        progress_tx: broadcast::Sender<DownloadProgressEvent>,
        queue_state: Arc<Mutex<QueueState>>,
        active: Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
        concurrent_page_downloads: usize,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<()> {
        let QueuedDownloadTask {
            id: task_id,
            task:
                DownloadTask {
                    chapter_id,
                    manga_id,
                    manga_title,
                    source_manager,
                    source_manga_id,
                    source_chapter_id,
                    save_path,
                    name,
                    library_path,
                    comic_info,
                },
            _permit: _capacity_permit,
        } = task;

        let safe_name = sanitize_filename(&name);

        if let Err(e) = tokio::fs::create_dir_all(&save_path).await {
            tracing::error!("Failed to create save directory {:?}: {}", save_path, e);
            Self::send_event(
                &progress_tx,
                DownloadProgressEvent::ChapterFailed {
                    chapter_id,
                    chapter_name: name.clone(),
                    error: format!("Failed to create save directory: {}", e),
                },
            );
            queue_state.lock().await.active_tasks.remove(&chapter_id);
            return Err(error::Error::Io(e));
        }

        let cbz_path = assert_within_root(
            &library_path,
            &save_path.join(format!("{}.cbz", &safe_name)),
        )?;
        let staging_dir = assert_within_root(
            &library_path,
            &save_path.join(format!(".tmp_staging_{}", task_id)),
        )?;

        if let Err(e) = tokio::fs::create_dir_all(&staging_dir).await {
            tracing::error!(
                "Failed to create staging directory {:?}: {}",
                staging_dir,
                e
            );
            Self::send_event(
                &progress_tx,
                DownloadProgressEvent::ChapterFailed {
                    chapter_id,
                    chapter_name: name.clone(),
                    error: format!("Failed to create staging directory: {}", e),
                },
            );

            queue_state.lock().await.active_tasks.remove(&chapter_id);
            return Err(error::Error::Io(e));
        }

        let fetch_fut = async {
            match source_manager.lease_instance().await {
                Ok(mut instance) => {
                    let pages = instance
                        .get_pages(&source_manga_id, &source_chapter_id)
                        .await?;

                    let metadata = instance.get_metadata().await?;

                    Ok((pages, metadata.base_url))
                }
                Err(e) => Err(e),
            }
        };

        let chapter_generated = tokio::select! {
            res = fetch_fut => match res {
                Ok((pages, base_url)) => Some((pages, base_url)),
                Err(e) => {
                    tracing::error!("Failed to get pages: {}", e);
                    Self::send_event(
                        &progress_tx,
                        DownloadProgressEvent::ChapterFailed {
                            chapter_id,
                            chapter_name: name.clone(),
                            error: format!("Failed to fetch pages: {}", e),
                        },
                    );
                    None
                }
            },
            _ = &mut cancel_rx => {
                Self::send_event(
                    &progress_tx,
                    DownloadProgressEvent::ChapterCancelled {
                        chapter_id,
                        chapter_name: name.clone(),
                    },
                );
                tracing::info!("Chapter '{}' cancelled during page fetch", name);
                None
            }
        };

        if let Some((chapter_generated, base_url)) = chapter_generated {
            let pages = chapter_generated.pages;
            let pages_len = pages.len();

            {
                let mut map = active.write().await;
                map.insert(
                    chapter_id,
                    ActiveDownloadState {
                        chapter_id,
                        chapter_name: name.clone(),
                        manga_id,
                        manga_title: manga_title.clone(),
                        total_pages: pages_len,
                        completed_pages: 0,
                        status: ActiveDownloadStatus::InProgress,
                    },
                );
            }

            Self::send_event(
                &progress_tx,
                DownloadProgressEvent::ChapterStarted {
                    chapter_id,
                    chapter_name: name.clone(),
                    manga_id,
                    manga_title: manga_title.clone(),
                    total_pages: pages_len,
                },
            );

            let active_for_stream = active.clone();

            let mut stream = stream::iter(pages)
                .map(|page| {
                    let client = client.clone();
                    let base_url = base_url.clone();
                    let name = name.clone();
                    let page_tx = progress_tx.clone();
                    let staging_dir = staging_dir.clone();
                    let active_for_page = active_for_stream.clone();

                    async move {
                        let result = Self::download_page_with_retry(
                            &client,
                            &page.url,
                            page.index,
                            &staging_dir,
                            max_retries,
                            initial_retry_delay_ms,
                            &base_url,
                        )
                        .await;

                        if result.is_ok() {
                            Self::update_active(&active_for_page, chapter_id, |s| {
                                s.completed_pages += 1;
                            })
                            .await;
                            Self::send_event(
                                &page_tx,
                                DownloadProgressEvent::PageCompleted {
                                    chapter_id,
                                    chapter_name: name.clone(),
                                    page_index: page.index,
                                },
                            );
                        }

                        result.map_err(|e| e.to_string())
                    }
                })
                .buffer_unordered(concurrent_page_downloads);

            let mut successful = Vec::new();
            let mut error = None;
            let mut is_cancelled = false;

            while let Some(result) = tokio::select! {
                res = stream.next() => res,
                _ = &mut cancel_rx => { is_cancelled = true; None }
            } {
                match result {
                    Ok(data) => successful.push(data),
                    Err(e) => {
                        error = Some(e);
                        break;
                    }
                }
            }

            if is_cancelled {
                Self::update_active(&active, chapter_id, |s| {
                    s.status = ActiveDownloadStatus::Cancelled;
                })
                .await;
                Self::send_event(
                    &progress_tx,
                    DownloadProgressEvent::ChapterCancelled {
                        chapter_id,
                        chapter_name: name.clone(),
                    },
                );
                tracing::info!("Chapter '{}' cancelled", name);
            } else if let Some(err_msg) = error {
                Self::update_active(&active, chapter_id, |s| {
                    s.status = ActiveDownloadStatus::Failed(err_msg.clone());
                })
                .await;
                Self::send_event(
                    &progress_tx,
                    DownloadProgressEvent::ChapterFailed {
                        chapter_id,
                        chapter_name: name.clone(),
                        error: format!("Page download failed: {}", err_msg),
                    },
                );
                tracing::warn!("Chapter '{}' failed: {}", name, err_msg);
            } else {
                let successful_len = successful.len();
                match Self::create_cbz(&cbz_path, successful, comic_info).await {
                    Ok(_) => {
                        Self::update_active(&active, chapter_id, |s| {
                            s.status = ActiveDownloadStatus::Completed;
                            s.completed_pages = successful_len;
                        })
                        .await;
                        Self::send_event(
                            &progress_tx,
                            DownloadProgressEvent::ChapterCompleted {
                                chapter_id,
                                chapter_name: name.clone(),
                                manga_id,
                                manga_title: manga_title.clone(),
                                successful_pages: successful_len,
                            },
                        );
                        tracing::info!("Chapter '{}' downloaded successfully", name);
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        Self::update_active(&active, chapter_id, |s| {
                            s.status = ActiveDownloadStatus::Failed(err_str.clone());
                        })
                        .await;
                        Self::send_event(
                            &progress_tx,
                            DownloadProgressEvent::ChapterFailed {
                                chapter_id,
                                chapter_name: name.clone(),
                                error: format!("Failed to create CBZ: {}", e),
                            },
                        );
                        tracing::error!("Failed to create CBZ for '{}': {}", name, e);
                    }
                }
            }

            Self::schedule_active_cleanup(active.clone(), chapter_id);
        }

        queue_state.lock().await.active_tasks.remove(&chapter_id);

        let mut delay = Duration::from_millis(200);
        let mut retries = 0u32;
        loop {
            match tokio::fs::remove_dir_all(&staging_dir).await {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => {
                    retries += 1;
                    if retries >= 5 {
                        tracing::warn!(
                            "Failed to clean up staging dir after {} attempts: {}",
                            retries,
                            e
                        );
                        return Err(error::Error::Io(e));
                    }
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    async fn make_manager() -> DownloaderManager {
        let client = crate::http::SmartClient::new(None).expect("SmartClient");
        DownloaderManager::new(client, 4, 2, 3, 100, 32)
            .await
            .expect("DownloaderManager")
    }

    // ── initial state ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn new_manager_queue_is_empty() {
        let mgr = make_manager().await;
        assert_eq!(mgr.queue_len().await, 0);
    }

    #[tokio::test]
    async fn list_queue_initially_empty() {
        let mgr = make_manager().await;
        assert!(mgr.list_queue().await.is_empty());
    }

    #[tokio::test]
    async fn snapshot_initially_empty() {
        let mgr = make_manager().await;
        assert!(mgr.snapshot().await.is_empty());
    }

    // ── operations on missing entries ────────────────────────────────────────

    #[tokio::test]
    async fn remove_unknown_id_returns_false() {
        let mgr = make_manager().await;
        assert!(!mgr.remove_from_queue(9999).await);
    }

    #[tokio::test]
    async fn cancel_nonexistent_chapter_returns_false() {
        let mgr = make_manager().await;
        assert!(!mgr.cancel_download(9999).await);
    }

    #[tokio::test]
    async fn cancel_already_completed_chapter_returns_false() {
        let mgr = make_manager().await;
        // Insert a fake completed entry into active map.
        mgr.active.write().await.insert(
            42,
            ActiveDownloadState {
                chapter_id: 42,
                chapter_name: "ch".into(),
                total_pages: 1,
                completed_pages: 1,
                status: ActiveDownloadStatus::Completed,
                manga_id: 1,
                manga_title: "manga".into(),
            },
        );
        assert!(!mgr.cancel_download(42).await);
    }

    // ── subscribe ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn subscribe_returns_receiver() {
        let mgr = make_manager().await;
        let mut rx = mgr.subscribe();
        // Receiver starts with no messages.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_broadcast() {
        let mgr = make_manager().await;
        let mut rx1 = mgr.subscribe();
        let mut rx2 = mgr.subscribe();

        let _ = mgr.progress_tx.send(DownloadProgressEvent::ChapterFailed {
            chapter_id: 1,
            chapter_name: "ch".into(),
            error: "test".into(),
        });

        assert!(rx1.recv().await.is_ok());
        assert!(rx2.recv().await.is_ok());
    }

    // ── queue id generation ──────────────────────────────────────────────────

    #[test]
    fn queue_state_ids_are_unique_and_incrementing() {
        let qs = QueueState::new();
        let id1 = qs.generate_id();
        let id2 = qs.generate_id();
        let id3 = qs.generate_id();
        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    #[test]
    fn queue_state_starts_at_one() {
        let qs = QueueState::new();
        assert_eq!(qs.generate_id(), 1);
    }

    // ── get_image_extension ──────────────────────────────────────────────────

    fn buffered_resp_with_ct(content_type: &'static str) -> SmartResponse {
        let mut headers = rquest::header::HeaderMap::new();
        headers.insert(
            rquest::header::CONTENT_TYPE,
            rquest::header::HeaderValue::from_static(content_type),
        );
        SmartResponse::Buffered {
            status: rquest::StatusCode::OK,
            url: rquest::Url::parse("https://example.com/img").unwrap(),
            headers,
            body: bytes::Bytes::new(),
        }
    }

    fn buffered_resp_no_ct() -> SmartResponse {
        SmartResponse::Buffered {
            status: rquest::StatusCode::OK,
            url: rquest::Url::parse("https://example.com/img").unwrap(),
            headers: rquest::header::HeaderMap::new(),
            body: bytes::Bytes::new(),
        }
    }

    #[test]
    fn image_extension_from_jpeg_content_type() {
        let resp = buffered_resp_with_ct("image/jpeg");
        assert_eq!(DownloaderManager::get_image_extension(&resp, "x"), "jpg");
    }

    #[test]
    fn image_extension_from_png_content_type() {
        let resp = buffered_resp_with_ct("image/png");
        assert_eq!(DownloaderManager::get_image_extension(&resp, "x"), "png");
    }

    #[test]
    fn image_extension_from_webp_content_type() {
        let resp = buffered_resp_with_ct("image/webp");
        assert_eq!(DownloaderManager::get_image_extension(&resp, "x"), "webp");
    }

    #[test]
    fn image_extension_from_url_when_no_content_type() {
        let resp = buffered_resp_no_ct();
        assert_eq!(
            DownloaderManager::get_image_extension(&resp, "https://cdn.example.com/page.png?v=1"),
            "png"
        );
    }

    #[test]
    fn image_extension_defaults_to_jpg_when_unknown() {
        let resp = buffered_resp_no_ct();
        assert_eq!(
            DownloaderManager::get_image_extension(&resp, "https://cdn.example.com/page"),
            "jpg"
        );
    }

    #[test]
    fn image_extension_gif_content_type() {
        let resp = buffered_resp_with_ct("image/gif");
        assert_eq!(DownloaderManager::get_image_extension(&resp, "x"), "gif");
    }

    // ── create_cbz ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_cbz_produces_valid_zip_with_pages() {
        let dir = tempfile::TempDir::new().unwrap();
        let page1 = dir.path().join("p1.tmp");
        let page2 = dir.path().join("p2.tmp");
        tokio::fs::write(&page1, b"fake-img-1").await.unwrap();
        tokio::fs::write(&page2, b"fake-img-2").await.unwrap();

        let cbz_path = dir.path().join("out.cbz");
        let staged = vec![
            (page1, "0001.jpg".to_string()),
            (page2, "0002.png".to_string()),
        ];
        DownloaderManager::create_cbz(&cbz_path, staged, None)
            .await
            .unwrap();

        let pages = crate::cbz::list_cbz_pages(&cbz_path).unwrap();
        assert_eq!(pages, vec!["0001.jpg", "0002.png"]);
    }

    #[tokio::test]
    async fn create_cbz_includes_comic_info_xml() {
        let dir = tempfile::TempDir::new().unwrap();
        let page = dir.path().join("p.tmp");
        tokio::fs::write(&page, b"data").await.unwrap();

        let cbz_path = dir.path().join("with_info.cbz");
        let info = crate::comic_info::ComicInfo {
            xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance",
            series: "Test Series".to_string(),
            title: None,
            number: 1.0,
            volume: None,
            summary: None,
            language_iso: None,
            writer: None,
            penciller: None,
            genre: None,
            web: None,
        };
        DownloaderManager::create_cbz(&cbz_path, vec![(page, "0001.jpg".to_string())], Some(info))
            .await
            .unwrap();

        // Verify ComicInfo.xml is present in the archive
        let file = std::fs::File::open(&cbz_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let entry_names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            entry_names.iter().any(|n| n == "ComicInfo.xml"),
            "ComicInfo.xml not found: {entry_names:?}"
        );
    }

    #[tokio::test]
    async fn create_cbz_without_comic_info_has_no_xml() {
        let dir = tempfile::TempDir::new().unwrap();
        let page = dir.path().join("p.tmp");
        tokio::fs::write(&page, b"data").await.unwrap();

        let cbz_path = dir.path().join("no_info.cbz");
        DownloaderManager::create_cbz(&cbz_path, vec![(page, "0001.jpg".to_string())], None)
            .await
            .unwrap();

        let file = std::fs::File::open(&cbz_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let entry_names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            !entry_names.iter().any(|n| n == "ComicInfo.xml"),
            "unexpected ComicInfo.xml"
        );
    }
}
