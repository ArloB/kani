//! Download manager for queuing and processing chapter downloads.

mod progress;

use futures::stream::{self, StreamExt};
use kani_shared::Chapter;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify};

use crate::error::Result;
use crate::http::{SmartClient, SmartResponse};
pub use progress::{DownloadProgress, ProgressEvent};

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

struct DownloadTask {
    id: QueueId,
    chapter: Chapter,
    save_path: PathBuf,
}

/// Shared queue state accessible from both the manager and worker
struct QueueState {
    queue: VecDeque<DownloadTask>,
    next_id: AtomicU64,
}

impl QueueState {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            next_id: AtomicU64::new(1),
        }
    }

    fn generate_id(&self) -> QueueId {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct DownloaderManager {
    queue: Arc<Mutex<QueueState>>,
    notify: Arc<Notify>,
}

impl DownloaderManager {
    pub fn new(
        solver_url: &str,
        concurrent_page_downloads: usize,
        _chapter_queue_size: usize, // Reserved for future use (e.g., backpressure limits)
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(QueueState::new()));
        let notify = Arc::new(Notify::new());

        let queue_clone = queue.clone();
        let notify_clone = notify.clone();
        let solver_url = if solver_url.is_empty() {
            None
        } else {
            Some(solver_url.to_string())
        };

        tokio::spawn(async move {
            let client = match SmartClient::new(solver_url) {
                Ok(client) => client,
                Err(e) => {
                    tracing::error!("Failed to create download client: {}", e);
                    return;
                }
            };

            loop {
                // Wait for notification or check queue
                let task = {
                    let mut state = queue_clone.lock().await;
                    state.queue.pop_front()
                };

                let Some(task) = task else {
                    // No tasks, wait for notification
                    notify_clone.notified().await;
                    continue;
                };

                let client = client.clone();
                let DownloadTask {
                    id: _task_id,
                    chapter,
                    save_path,
                } = task;

                let chapter_name = chapter.chapter_name.clone();
                let chapter_path = save_path.join(&chapter_name);

                if let Err(e) = tokio::fs::create_dir_all(&chapter_path).await {
                    tracing::error!(
                        "Failed to create chapter directory {:?}: {}",
                        chapter_path,
                        e
                    );
                    Self::on_chapter_failed(&chapter_name, &e.to_string());
                    continue;
                }

                Self::on_chapter_started(&chapter_name, chapter.pages.len());

                let results: Vec<std::result::Result<(), String>> =
                    stream::iter(chapter.pages.into_iter())
                        .map(|page| {
                            let client = client.clone();
                            let chapter_path = chapter_path.clone();
                            let chapter_name = chapter_name.clone();

                            async move {
                                let result = Self::download_page_with_retry(
                                    &client,
                                    &page.url,
                                    chapter_path,
                                    page.index,
                                    max_retries,
                                    initial_retry_delay_ms,
                                )
                                .await;

                                match &result {
                                    Ok(_) => Self::on_page_completed(&chapter_name, page.index),
                                    Err(e) => Self::on_page_failed(
                                        &chapter_name,
                                        page.index,
                                        &e.to_string(),
                                    ),
                                }

                                result.map_err(|e| e.to_string())
                            }
                        })
                        .buffer_unordered(concurrent_page_downloads)
                        .collect()
                        .await;

                let (successful, failed): (Vec<_>, Vec<_>) =
                    results.into_iter().partition(std::result::Result::is_ok);

                if failed.is_empty() {
                    Self::on_chapter_completed(&chapter_name, successful.len());
                    tracing::info!(
                        "Chapter '{}' completed: {} pages downloaded",
                        chapter_name,
                        successful.len()
                    );
                } else {
                    Self::on_chapter_failed(
                        &chapter_name,
                        &format!("{} pages failed", failed.len()),
                    );
                    tracing::warn!(
                        "Chapter '{}' completed with errors: {}/{} pages successful",
                        chapter_name,
                        successful.len(),
                        successful.len() + failed.len()
                    );
                }
            }
        });

        Ok(Self { queue, notify })
    }

    /// Queue a chapter for download, returns the queue ID
    pub async fn queue_chapter(&self, chapter: Chapter, save_path: PathBuf) -> Result<QueueId> {
        let id = {
            let mut state = self.queue.lock().await;
            let id = state.generate_id();
            state.queue.push_back(DownloadTask {
                id,
                chapter,
                save_path,
            });
            id
        };

        // Notify the worker that there's a new task
        self.notify.notify_one();

        Ok(id)
    }

    /// List all chapters currently in the queue (not including the one being downloaded)
    pub async fn list_queue(&self) -> Vec<QueuedChapter> {
        let state = self.queue.lock().await;
        state
            .queue
            .iter()
            .map(|task| QueuedChapter {
                id: task.id,
                chapter_name: task.chapter.chapter_name.clone(),
                page_count: task.chapter.pages.len(),
                save_path: task.save_path.clone(),
            })
            .collect()
    }

    /// Remove a chapter from the queue by its ID
    /// Returns true if the chapter was found and removed, false otherwise
    pub async fn remove_from_queue(&self, id: QueueId) -> bool {
        let mut state = self.queue.lock().await;
        if let Some(pos) = state.queue.iter().position(|task| task.id == id) {
            state.queue.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get the number of chapters currently in the queue
    pub async fn queue_len(&self) -> usize {
        self.queue.lock().await.queue.len()
    }

    async fn download_page_with_retry(
        client: &SmartClient,
        url: &str,
        chapter_path: PathBuf,
        page_index: i32,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<()> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(initial_retry_delay_ms.try_into()?);

        loop {
            match Self::download_page(client, url, &chapter_path, page_index).await {
                Ok(_) => return Ok(()),
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
        chapter_path: &std::path::Path,
        page: i32,
    ) -> Result<()> {
        let resp = client.get(url).await?;

        let extension = Self::get_image_extension(&resp, url);

        let body = resp.bytes().await?;
        tokio::fs::write(chapter_path.join(format!("{}.{}", page, extension)), body).await?;

        Ok(())
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

    // ============================================================
    // Progress Tracking Skeleton Functions
    // ============================================================

    fn on_chapter_started(_chapter_name: &str, _total_pages: usize) {
        // TODO: Emit ProgressEvent::ChapterStarted
    }

    fn on_chapter_completed(_chapter_name: &str, _pages_downloaded: usize) {
        // TODO: Emit ProgressEvent::ChapterCompleted
    }

    fn on_chapter_failed(_chapter_name: &str, _error: &str) {
        // TODO: Emit ProgressEvent::ChapterFailed
    }

    fn on_page_completed(_chapter_name: &str, _page_index: i32) {
        // TODO: Emit ProgressEvent::PageCompleted
    }

    fn on_page_failed(_chapter_name: &str, _page_index: i32, _error: &str) {
        // TODO: Emit ProgressEvent::PageFailed
    }
}
