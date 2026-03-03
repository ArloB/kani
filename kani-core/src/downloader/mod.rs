//! Download manager for queuing and processing chapter downloads.

mod progress;

use crate::error::Result;
use crate::http::{SmartClient, SmartResponse};
use crate::sanitize::sanitize_filename;
use futures::stream::{self, StreamExt};
use kani_shared::{Chapter, DownloadProgressEvent};
pub use progress::{DownloadProgress, ProgressEvent};
use std::collections::VecDeque;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify, broadcast, oneshot};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Channel capacity for download progress events.
/// Slow subscribers will lose events once the buffer fills — that's acceptable for progress UI.
const PROGRESS_CHANNEL_CAPACITY: usize = 64;

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
    name: String,
    save_path: PathBuf,
    comic_info_xml: Option<String>,
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
    progress_tx: broadcast::Sender<DownloadProgressEvent>,
}

impl DownloaderManager {
    pub fn subscribe(&self) -> broadcast::Receiver<DownloadProgressEvent> {
        self.progress_tx.subscribe()
    }
}

impl DownloaderManager {
    pub async fn new(
        solver_url: &str,
        concurrent_page_downloads: usize,
        _chapter_queue_size: usize, // Reserved for future use (e.g., backpressure limits)
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(QueueState::new()));
        let notify = Arc::new(Notify::new());
        let (tx, rx) = oneshot::channel::<Result<()>>();
        let (progress_tx, _) = broadcast::channel(PROGRESS_CHANNEL_CAPACITY);

        let queue_clone = queue.clone();
        let notify_clone = notify.clone();
        let progress_tx_clone = progress_tx.clone();
        let solver_url = if solver_url.is_empty() {
            None
        } else {
            Some(solver_url.to_string())
        };

        tokio::spawn(async move {
            let client = match SmartClient::new(solver_url) {
                Ok(client) => {
                    let _ = tx.send(Ok(()));
                    client
                }
                Err(e) => {
                    tracing::error!("Failed to create download client: {}", e);
                    let _ = tx.send(Err(crate::error::Error::Internal(format!(
                        "Failed to create download client: {}",
                        e
                    ))));
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
                let progress_tx = progress_tx_clone.clone();
                let DownloadTask {
                    id: _task_id,
                    chapter,
                    save_path,
                    name,
                    comic_info_xml,
                } = task;

                let safe_name = sanitize_filename(&name);

                let cbz_path = save_path.join(format!("{}.cbz", &safe_name));

                if let Err(e) = tokio::fs::create_dir_all(&save_path).await {
                    tracing::error!("Failed to create base directory {:?}: {}", save_path, e);
                    Self::send_event(
                        &progress_tx,
                        DownloadProgressEvent::ChapterFailed {
                            chapter_name: name.clone(),
                            error: e.to_string(),
                        },
                    );
                    continue;
                }

                Self::send_event(
                    &progress_tx,
                    DownloadProgressEvent::ChapterStarted {
                        chapter_name: name.clone(),
                        total_pages: chapter.pages.len(),
                    },
                );

                let results: Vec<std::result::Result<(String, Vec<u8>), String>> =
                    stream::iter(chapter.pages.into_iter())
                        .map(|page| {
                            let client = client.clone();
                            let name = name.clone();
                            let page_tx = progress_tx.clone();

                            async move {
                                let result = Self::download_page_with_retry(
                                    &client,
                                    &page.url,
                                    page.index,
                                    max_retries,
                                    initial_retry_delay_ms,
                                )
                                .await;

                                match &result {
                                    Ok(_) => Self::send_event(
                                        &page_tx,
                                        DownloadProgressEvent::PageCompleted {
                                            chapter_name: name.clone(),
                                            page_index: page.index,
                                        },
                                    ),
                                    Err(e) => Self::send_event(
                                        &page_tx,
                                        DownloadProgressEvent::PageFailed {
                                            chapter_name: name.clone(),
                                            page_index: page.index,
                                            error: e.to_string(),
                                        },
                                    ),
                                }

                                result.map_err(|e| e.to_string())
                            }
                        })
                        .buffer_unordered(concurrent_page_downloads)
                        .collect()
                        .await;

                let (mut successful, failed): (Vec<_>, Vec<_>) =
                    results.into_iter().partition(std::result::Result::is_ok);

                if failed.is_empty() {
                    successful.sort_by(|a, b| {
                        let a_val = a.as_ref().unwrap();
                        let b_val = b.as_ref().unwrap();
                        a_val.0.cmp(&b_val.0)
                    });

                    let mut archive = Cursor::new(Vec::new());
                    {
                        let mut zip = ZipWriter::new(&mut archive);
                        let options = SimpleFileOptions::default()
                            .compression_method(zip::CompressionMethod::Stored);

                        let mut zip_failed = false;

                        if let Some(ref xml_content) = comic_info_xml
                            && zip.start_file("ComicInfo.xml", options).is_ok() {
                                let _ = zip.write_all(xml_content.as_bytes());
                            }

                        for (filename, bytes) in successful.iter().flatten() {
                            if let Err(e) = zip.start_file(filename, options) {
                                tracing::error!("Failed to start file in zip: {}", e);
                                zip_failed = true;
                                break;
                            }
                            if let Err(e) = zip.write_all(bytes) {
                                tracing::error!("Failed to write to zip: {}", e);
                                zip_failed = true;
                                break;
                            }
                        }

                        if !zip_failed && zip.finish().is_err() {
                            tracing::error!("Failed to finish zip archive");
                            zip_failed = true;
                        }

                        if zip_failed {
                            Self::send_event(
                                &progress_tx,
                                DownloadProgressEvent::ChapterFailed {
                                    chapter_name: name.clone(),
                                    error: "Failed to assemble zip archive".to_string(),
                                },
                            );
                            continue;
                        }
                    }

                    if let Err(e) = tokio::fs::write(&cbz_path, archive.into_inner()).await {
                        tracing::error!("Failed to save .cbz file {:?}: {}", cbz_path, e);
                        Self::send_event(
                            &progress_tx,
                            DownloadProgressEvent::ChapterFailed {
                                chapter_name: name.clone(),
                                error: e.to_string(),
                            },
                        );
                        continue;
                    }

                    Self::send_event(
                        &progress_tx,
                        DownloadProgressEvent::ChapterCompleted {
                            chapter_name: name.clone(),
                            successful_pages: successful.len(),
                            failed_pages: 0,
                        },
                    );
                    tracing::info!(
                        "Chapter '{}' completed: {} pages downloaded to .cbz",
                        name,
                        successful.len()
                    );
                } else {
                    Self::send_event(
                        &progress_tx,
                        DownloadProgressEvent::ChapterFailed {
                            chapter_name: name.clone(),
                            error: format!("{} pages failed", failed.len()),
                        },
                    );
                    tracing::warn!(
                        "Chapter '{}' completed with errors: {}/{} pages successful",
                        name,
                        successful.len(),
                        successful.len() + failed.len()
                    );
                }
            }
        });

        rx.await.map_err(|_| {
            crate::error::Error::Internal(
                "Downloader worker task panicked during initialization".to_string(),
            )
        })??;

        Ok(Self {
            queue,
            notify,
            progress_tx,
        })
    }

    /// Queue a chapter for download, returns the queue ID
    pub async fn queue_chapter(
        &self,
        chapter: Chapter,
        name: String,
        save_path: PathBuf,
        comic_info_xml: Option<String>,
    ) -> Result<QueueId> {
        let id = {
            let mut state = self.queue.lock().await;
            let id = state.generate_id();
            state.queue.push_back(DownloadTask {
                id,
                chapter,
                name,
                save_path,
                comic_info_xml,
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
                chapter_name: task.name.clone(),
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
        page_index: i32,
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<(String, Vec<u8>)> {
        let mut attempts = 0;
        let mut delay = Duration::from_millis(initial_retry_delay_ms.try_into()?);

        loop {
            match Self::download_page(client, url, page_index).await {
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
    ) -> Result<(String, Vec<u8>)> {
        let resp = client.get(url).await?;

        let extension = Self::get_image_extension(&resp, url);

        let body = resp.bytes().await?;
        let filename = format!("{:04}.{}", page, extension);

        Ok((filename, body.to_vec()))
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
    // Internal helpers
    // ============================================================

    /// Send an event on the progress broadcast channel.
    ///
    /// A `SendError` simply means no subscribers are currently connected — this is
    /// perfectly normal and is silently ignored.
    fn send_event(tx: &broadcast::Sender<DownloadProgressEvent>, event: DownloadProgressEvent) {
        let _ = tx.send(event);
    }
}
