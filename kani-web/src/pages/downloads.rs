use kani_shared::DownloadProgressEvent;
use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_sys::{EventSource, MessageEvent};

/// Live per-chapter progress tracking for downloads.
#[derive(Debug, Clone, PartialEq)]
struct ChapterProgress {
    pub name: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub failed_pages: usize,
    pub status: ChapterStatus,
}

#[derive(Debug, Clone, PartialEq)]
enum ChapterStatus {
    InProgress,
    Completed,
    Failed(String),
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
    let chapters = RwSignal::new(HashMap::<String, ChapterProgress>::new());

    Effect::new(move |_| {
        let es = match EventSource::new("/api/downloads/progress") {
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

            chapters_signal.update(|map| match event {
                DownloadProgressEvent::ChapterStarted {
                    chapter_name,
                    total_pages,
                } => {
                    map.insert(
                        chapter_name.clone(),
                        ChapterProgress {
                            name: chapter_name,
                            total_pages,
                            completed_pages: 0,
                            failed_pages: 0,
                            status: ChapterStatus::InProgress,
                        },
                    );
                }
                DownloadProgressEvent::PageCompleted { chapter_name, .. } => {
                    if let Some(c) = map.get_mut(&chapter_name) {
                        c.completed_pages += 1;
                    }
                }
                DownloadProgressEvent::PageFailed { chapter_name, .. } => {
                    if let Some(c) = map.get_mut(&chapter_name) {
                        c.failed_pages += 1;
                    }
                }
                DownloadProgressEvent::ChapterCompleted {
                    chapter_name,
                    successful_pages,
                    failed_pages,
                    ..
                } => {
                    if let Some(c) = map.get_mut(&chapter_name) {
                        c.completed_pages = successful_pages;
                        c.failed_pages = failed_pages;
                        c.status = ChapterStatus::Completed;
                    }
                }
                DownloadProgressEvent::ChapterFailed {
                    chapter_name,
                    error,
                } => {
                    if let Some(c) = map.get_mut(&chapter_name) {
                        c.status = ChapterStatus::Failed(error);
                    }
                }
            });
        });

        es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let es_clone = es.clone();
        on_cleanup(move || {
            es_clone.close();
        });

        drop(es);
    });

    let dismiss = move |name: String| {
        chapters.update(|map| {
            map.remove(&name);
        });
    };

    view! {
        {move || {
            let map = chapters.get();
            if map.is_empty() {
                return view! { <div></div> }.into_any();
            }

            let mut entries: Vec<ChapterProgress> = map.into_values().collect();
            entries.sort_by_key(|c| matches!(c.status, ChapterStatus::InProgress));
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
                                let dismiss_name = name.clone();
                                let is_done = !matches!(chapter.status, ChapterStatus::InProgress);
                                let bar_class = match &chapter.status {
                                    ChapterStatus::InProgress => "progress-bar progress-bar--active",
                                    ChapterStatus::Completed => "progress-bar progress-bar--done",
                                    ChapterStatus::Failed(_) => "progress-bar progress-bar--failed",
                                };
                                let status_text = match &chapter.status {
                                    ChapterStatus::InProgress => format!(
                                        "{}/{} pages",
                                        chapter.completed_pages + chapter.failed_pages,
                                        chapter.total_pages
                                    ),
                                    ChapterStatus::Completed => "Complete".to_string(),
                                    ChapterStatus::Failed(e) => format!("Failed: {e}"),
                                };
                                view! {
                                    <div class="download-item">
                                        <div class="download-item-header">
                                            <span class="download-item-name">{name}</span>
                                            <span class="download-item-status">{status_text}</span>
                                            {is_done.then(|| view! {
                                                <button
                                                    class="download-item-dismiss"
                                                    on:click=move |_| dismiss(dismiss_name.clone())
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
