use leptos::prelude::*;
use std::collections::HashMap;
use crate::types::LiveChapterStatus;

/// Live per-chapter progress tracking for downloads.
#[derive(Debug, Clone, PartialEq)]
struct ChapterProgress {
    pub id: i64,
    pub name: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub failed_pages: usize,
    pub status: LiveChapterStatus,
}

impl ChapterProgress {
    fn completion_pct(&self) -> f64 {
        if self.total_pages == 0 {
            return 100.0;
        }
        (self.completed_pages + self.failed_pages) as f64 / self.total_pages as f64 * 100.0
    }
}

#[component]
pub fn DownloadProgress() -> impl IntoView {
    let chapters = RwSignal::new(HashMap::<i64, ChapterProgress>::new());

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use crate::events::DownloadProgressEvent;

            use wasm_bindgen::prelude::*;
            use web_sys::{EventSource, MessageEvent};

            let es = match EventSource::new("/rest/downloads/progress") {
                Ok(es) => es,
                Err(e) => {
                    log::error!("Failed to open EventSource: {:?}", e);
                    return;
                }
            };

            let chapters_signal = chapters;
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |msg: MessageEvent| {
                let data = match msg.data().as_string() {
                    Some(d) => d,
                    None => return,
                };

                let event: DownloadProgressEvent = match serde_json::from_str(&data) {
                    Ok(e) => e,
                    Err(e) => {
                        log::warn!("Failed to parse download event: {e}");
                        return;
                    }
                };

                let maybe_dismiss_id = match &event {
                    DownloadProgressEvent::ChapterCompleted { chapter_id, .. } => Some(*chapter_id),
                    DownloadProgressEvent::ChapterFailed { chapter_id, .. } => Some(*chapter_id),
                    DownloadProgressEvent::ChapterCancelled { chapter_id, .. } => Some(*chapter_id),
                    _ => None,
                };

                chapters_signal.update(|map| match event {
                    DownloadProgressEvent::ChapterStarted {
                        chapter_id,
                        chapter_name,
                        total_pages,
                    } => {
                        map.insert(
                            chapter_id,
                            ChapterProgress {
                                id: chapter_id,
                                name: chapter_name,
                                total_pages,
                                completed_pages: 0,
                                failed_pages: 0,
                                status: LiveChapterStatus::InProgress,
                            },
                        );
                    }
                    DownloadProgressEvent::PageCompleted { chapter_id, .. } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.completed_pages += 1;
                        }
                    }
                    DownloadProgressEvent::ChapterCompleted {
                        chapter_id,
                        successful_pages,
                        failed_pages,
                        ..
                    } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.completed_pages = successful_pages;
                            c.failed_pages = failed_pages;
                            c.status = LiveChapterStatus::Completed;
                        }
                    }
                    DownloadProgressEvent::ChapterFailed {
                        chapter_id,
                        error,
                        ..
                    } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.status = LiveChapterStatus::Failed(error);
                        }
                    }
                    DownloadProgressEvent::ChapterCancelled {
                        chapter_id,
                        ..
                    } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.status = LiveChapterStatus::Cancelled;
                        }
                    }
                });

                if let Some(id) = maybe_dismiss_id {
                    let s = chapters_signal;
                    set_timeout(
                        move || {
                            s.update(|m| {
                                m.remove(&id);
                            });
                        },
                        std::time::Duration::from_secs(5),
                    );
                }
            });

            es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();

            let es_clone = es.clone();
            on_cleanup(move || {
                es_clone.close();
            });

            drop(es);
        }
    });

    let dismiss = move |id: i64| {
        chapters.update(|map| {
            map.remove(&id);
        });
    };

    view! {
        {move || {
            let map = chapters.get();
            if map.is_empty() {
                return view! { <div></div> }.into_any();
            }

            let mut entries: Vec<ChapterProgress> = map.into_values().collect();
            entries.sort_by_key(|c| matches!(c.status, LiveChapterStatus::InProgress));
            entries.reverse();

            view! {
                <div class="download-progress-overlay">
                    <div class="download-progress-header">
                        <span class="download-progress-title">"Downloads"</span>
                    </div>
                    <div class="download-progress-list">
                        <For
                            each=move || entries.clone()
                            key=|c| c.name.clone()
                            children=move |chapter| {
                                let pct = chapter.completion_pct();
                                let name = chapter.name.clone();
                                let dismiss_id = chapter.id;
                                let is_done = !matches!(chapter.status, LiveChapterStatus::InProgress);
                                let bar_class = match &chapter.status {
                                    LiveChapterStatus::InProgress=>"progress-bar progress-bar--active",
                                    LiveChapterStatus::Completed=>"progress-bar progress-bar--done",
                                    LiveChapterStatus::Failed(_)=>"progress-bar progress-bar--failed",
                                    LiveChapterStatus::Cancelled=>"progress-bar progress-bar--failed",
                                    LiveChapterStatus::Deleted => todo!(),
                                                                    };
                                let status_text = match &chapter.status {
                                    LiveChapterStatus::InProgress => format!(
                                        "{}/{} pages",
                                        chapter.completed_pages + chapter.failed_pages,
                                        chapter.total_pages
                                    ),
                                    LiveChapterStatus::Completed  => "Complete".to_string(),
                                    LiveChapterStatus::Failed(e)  => format!("Failed: {e}"),
                                    LiveChapterStatus::Cancelled  => "Cancelled".to_string(),
                                    LiveChapterStatus::Deleted => todo!(),
                                };
                                view! {
                                    <div class="download-item">
                                        <div class="download-item-header">
                                            <span class="download-item-name">{name}</span>
                                            <span class="download-item-status">{status_text}</span>
                                            {is_done.then(|| view! {
                                                <button
                                                    class="download-item-dismiss"
                                                    on:click=move |_| dismiss(dismiss_id)
                                                >
                                                    "✕"
                                                </button>
                                            })}
                                        </div>
                                        <div class="progress-track">
                                            <div
                                                class=bar_class
                                                style=format!("width: {:.1}%", pct)
                                            ></div>
                                        </div>
                                    </div>
                                }
                            }
                        />
                    </div>
                </div>
            }.into_any()
        }}
    }
}
