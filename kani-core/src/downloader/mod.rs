//! Download manager for queuing and processing chapter downloads.

mod progress;

use crate::error::Result;
use crate::http::{SmartClient, SmartResponse};
use crate::sanitize::sanitize_filename;
use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::stream::{self, StreamExt};
use kani_shared::DownloadProgressEvent;
pub use progress::{DownloadProgress, ProgressEvent};
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
    pub source_manager: Arc<crate::source_manager::SourceManager>,
    pub source_manga_id: String,
    pub source_chapter_id: String,
    pub name: String,
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
    pub chapter_id:      i64,
    pub chapter_name:    String,
    pub total_pages:     usize,
    pub completed_pages: usize,
    pub status:          ActiveDownloadStatus,
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
        let queue              = Arc::new(Mutex::new(QueueState::new()));
        let (progress_tx, _)   = broadcast::channel(PROGRESS_CHANNEL_CAPACITY);
        let active             = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let capacity_semaphore = Arc::new(tokio::sync::Semaphore::new(queue_limit));
        let queue_semaphore    = Arc::new(tokio::sync::Semaphore::new(0)); // starts empty
        let chapter_limiter    = Arc::new(tokio::sync::Semaphore::new(concurrent_chapters));

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
        let permit = self.capacity_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        let id = {
            let mut state = self.queue.lock().await;
            let id = state.generate_id();
            state.queue.push_back(QueuedDownloadTask { id, task, _permit: permit });
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

        zip_writer
            .close()
            .await
            .map_err(|e| crate::error::Error::Other(format!("Zip error: {}", e)))?;

        Ok(())
    }

    async fn download_page_with_retry(
        client: &SmartClient,
        url: &str,
        page_index: i32,
        staging_dir: &std::path::Path,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<(PathBuf, String)> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(initial_retry_delay_ms.try_into()?);

        loop {
            match Self::download_page(client, url, page_index, staging_dir).await {
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
    ) -> Result<(PathBuf, String)> {
        let mut resp = client.get(url).await?;

        let extension = Self::get_image_extension(&resp, url);

        let filename = format!("{:04}.{}", page, extension);
        let tmp_file_path = staging_dir.join(format!("{:04}.tmp", page));

        let mut file = tokio::fs::File::create(&tmp_file_path).await?;

        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
        }
        file.flush().await?;

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

        if let Some(ext) = url.rsplit('.').next() {
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
            let queue_permit = queue_semaphore
                .clone()
                .acquire_owned()
                .await
                .expect("queue semaphore closed");
            queue_permit.forget();

            let task = queue_state
                .lock()
                .await
                .queue
                .pop_front()
                .expect("semaphore guarantees an item exists");

            let concurrency_permit = chapter_limiter
                .clone()
                .acquire_owned()
                .await
                .expect("chapter limiter closed");

            let client      = client_global.clone();
            let tx          = progress_tx.clone();
            let queue_ref   = queue_state.clone();
            let active_ref  = active.clone();

            tokio::spawn(async move {
                let _concurrency_permit = concurrency_permit;
                Self::process_chapter(
                    task,
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

    async fn process_chapter(
        task: QueuedDownloadTask,
        client: SmartClient,
        progress_tx: broadcast::Sender<DownloadProgressEvent>,
        queue_state: Arc<Mutex<QueueState>>,
        active: Arc<tokio::sync::RwLock<HashMap<i64, ActiveDownloadState>>>,
        concurrent_page_downloads: usize,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) {
        let QueuedDownloadTask {
            id: task_id,
            task: DownloadTask {
                chapter_id,
                source_manager,
                source_manga_id,
                source_chapter_id,
                save_path,
                name,
                comic_info,
            },
            _permit: _capacity_permit,
        } = task;

        let safe_name   = sanitize_filename(&name);
        let cbz_path    = save_path.join(format!("{}.cbz", &safe_name));
        let staging_dir = save_path.join(format!(".tmp_staging_{}", task_id));

        if let Err(e) = tokio::fs::create_dir_all(&staging_dir).await {
            tracing::error!("Failed to create staging directory {:?}: {}", staging_dir, e);
            Self::send_event(
                &progress_tx,
                DownloadProgressEvent::ChapterFailed {
                    chapter_id,
                    chapter_name: name.clone(),
                    error: format!("Failed to create staging directory: {}", e),
                },
            );
            return;
        }

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        queue_state.lock().await.active_tasks.insert(chapter_id, cancel_tx);

        let fetch_fut = async {
            match source_manager.lease_instance().await {
                Ok(mut instance) => instance.get_pages(&source_manga_id, &source_chapter_id).await,
                Err(e) => Err(crate::error::Error::Internal(format!(
                    "Failed to lease instance: {}", e
                ))),
            }
        };

        let chapter_generated = tokio::select! {
            res = fetch_fut => match res {
                Ok(pages) => Some(pages),
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

        if let Some(chapter_generated) = chapter_generated {
            let pages     = chapter_generated.pages;
            let pages_len = pages.len();

            {
                let mut map = active.write().await;
                map.insert(chapter_id, ActiveDownloadState {
                    chapter_id,
                    chapter_name: name.clone(),
                    total_pages: pages_len,
                    completed_pages: 0,
                    status: ActiveDownloadStatus::InProgress,
                });
            }

            Self::send_event(
                &progress_tx,
                DownloadProgressEvent::ChapterStarted {
                    chapter_id,
                    chapter_name: name.clone(),
                    total_pages: pages_len,
                },
            );

            let active_for_stream = active.clone();

            let mut stream = stream::iter(pages.into_iter())
                .map(|page| {
                    let client          = client.clone();
                    let name            = name.clone();
                    let page_tx         = progress_tx.clone();
                    let staging_dir     = staging_dir.clone();
                    let active_for_page = active_for_stream.clone();

                    async move {
                        let result = Self::download_page_with_retry(
                            &client, &page.url, page.index, &staging_dir,
                            max_retries, initial_retry_delay_ms,
                        ).await;

                        if result.is_ok() {
                            Self::update_active(&active_for_page, chapter_id, |s| {
                                s.completed_pages += 1;
                            }).await;
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

            let mut successful   = Vec::new();
            let mut error        = None;
            let mut is_cancelled = false;

            while let Some(result) = tokio::select! {
                res = stream.next() => res,
                _ = &mut cancel_rx => { is_cancelled = true; None }
            } {
                match result {
                    Ok(data) => successful.push(data),
                    Err(e)   => { error = Some(e); break; }
                }
            }

            if is_cancelled {
                Self::update_active(&active, chapter_id, |s| {
                    s.status = ActiveDownloadStatus::Cancelled;
                }).await;
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
                }).await;
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
                        }).await;
                        Self::send_event(
                            &progress_tx,
                            DownloadProgressEvent::ChapterCompleted {
                                chapter_id,
                                chapter_name: name.clone(),
                                successful_pages: successful_len,
                            },
                        );
                        tracing::info!("Chapter '{}' downloaded successfully", name);
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        Self::update_active(&active, chapter_id, |s| {
                            s.status = ActiveDownloadStatus::Failed(err_str.clone());
                        }).await;
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

        let mut delay   = Duration::from_millis(200);
        let mut retries = 0u32;
        loop {
            match tokio::fs::remove_dir_all(&staging_dir).await {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => {
                    retries += 1;
                    if retries >= 5 {
                        tracing::warn!(
                            "Failed to clean up staging dir after {} attempts: {}", retries, e
                        );
                        break;
                    }
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
            }
        }
    }
}
