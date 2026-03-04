//! Download manager for queuing and processing chapter downloads.

mod progress;

use crate::error::Result;
use crate::http::{SmartClient, SmartResponse};
use crate::sanitize::sanitize_filename;
use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use futures::stream::{self, StreamExt};
use kani_shared::{Chapter, DownloadProgressEvent};
pub use progress::{DownloadProgress, ProgressEvent};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, Notify, broadcast};
use tokio_util::compat::{FuturesAsyncWriteCompatExt, TokioAsyncWriteCompatExt};

/// Channel capacity for download progress events.
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
    chapter_id: i64,
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
        smart_client: SmartClient,
        concurrent_page_downloads: usize,
        _chapter_queue_size: usize, // Reserved for future use (e.g., backpressure limits)
        max_retries: i64,
        initial_retry_delay_ms: i64,
    ) -> Result<Self> {
        let queue = Arc::new(Mutex::new(QueueState::new()));
        let notify = Arc::new(Notify::new());
        let (progress_tx, _) = broadcast::channel(PROGRESS_CHANNEL_CAPACITY);

        let queue_clone = queue.clone();
        let notify_clone = notify.clone();
        let progress_tx_clone = progress_tx.clone();

        tokio::spawn(async move {
            let client = smart_client;

            loop {
                let task = {
                    let mut state = queue_clone.lock().await;
                    state.queue.pop_front()
                };

                let Some(task) = task else {
                    notify_clone.notified().await;
                    continue;
                };

                let client = client.clone();
                let progress_tx = progress_tx_clone.clone();
                let DownloadTask {
                    id: _task_id,
                    chapter_id,
                    chapter,
                    save_path,
                    name,
                    comic_info_xml,
                } = task;

                let safe_name = sanitize_filename(&name);

                let cbz_path = save_path.join(format!("{}.cbz", &safe_name));
                let staging_dir = save_path.join(format!(".tmp_staging_{}", _task_id));

                if let Err(e) = tokio::fs::create_dir_all(&staging_dir).await {
                    tracing::error!(
                        "Failed to create staging directory {:?}: {}",
                        staging_dir,
                        e
                    );
                    Self::send_event(
                        &progress_tx,
                        DownloadProgressEvent::ChapterFailed {
                            chapter_name: name.clone(),
                            error: format!("Failed to create staging directory: {}", e),
                        },
                    );
                    continue;
                }

                Self::send_event(
                    &progress_tx,
                    DownloadProgressEvent::ChapterStarted {
                        chapter_id,
                        chapter_name: name.clone(),
                        total_pages: chapter.pages.len(),
                    },
                );

                let results: Vec<std::result::Result<(PathBuf, String), String>> =
                    stream::iter(chapter.pages.into_iter())
                        .map(|page| {
                            let client = client.clone();
                            let name = name.clone();
                            let page_tx = progress_tx.clone();
                            let staging_dir = staging_dir.clone();

                            async move {
                                let result = Self::download_page_with_retry(
                                    &client,
                                    &page.url,
                                    page.index,
                                    &staging_dir,
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
                        a_val.1.cmp(&b_val.1)
                    });

                    let successful_len = successful.len();

                    match Self::create_cbz(
                        &cbz_path,
                        successful.into_iter().map(|res| res.unwrap()).collect(),
                        &comic_info_xml,
                    )
                    .await
                    {
                        Ok(_) => {
                            Self::send_event(
                                &progress_tx,
                                DownloadProgressEvent::ChapterCompleted {
                                    chapter_id,
                                    chapter_name: name.clone(),
                                    successful_pages: successful_len,
                                    failed_pages: 0,
                                },
                            );
                            tracing::info!(
                                "Chapter '{}' completed: {} pages downloaded to .cbz",
                                name,
                                successful_len
                            );
                        }
                        Err(e) => {
                            tracing::error!("Failed to assemble zip archive: {}", e);
                            Self::send_event(
                                &progress_tx,
                                DownloadProgressEvent::ChapterFailed {
                                    chapter_name: name.clone(),
                                    error: e.to_string(),
                                },
                            );
                        }
                    }
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

                if staging_dir.exists() {
                    let _ = tokio::fs::remove_dir_all(&staging_dir).await;
                }
            }
        });

        Ok(Self {
            queue,
            notify,
            progress_tx,
        })
    }

    /// Queue a chapter for download, returns the queue ID
    pub async fn queue_chapter(
        &self,
        chapter_id: i64,
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
                chapter_id,
                chapter,
                name,
                save_path,
                comic_info_xml,
            });
            id
        };

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
        comic_info_xml: &Option<String>,
    ) -> Result<()> {
        let cbz_file = tokio::fs::File::create(cbz_path).await?;
        let mut zip_writer = ZipFileWriter::new(cbz_file.compat_write());

        if let Some(xml_content) = comic_info_xml {
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
}
