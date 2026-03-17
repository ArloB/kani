use crate::pages::downloads::DownloadProgress;
use crate::pages::settings::Settings;
use crate::pages::sources::Sources;
use crate::pages::library::Library;
use crate::pages::manga_details::MangaDetails;
use crate::pages::source_details::SourceDetails;
use crate::pages::global_search::GlobalSearch;
use crate::pages::recent_updates::RecentUpdates;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

#[derive(Debug, Clone, PartialEq)]
pub struct ScanNotification {
    pub manga_id:   i64,
    pub manga_name: String,
    pub count:      usize,
}

#[component]
fn ScanBadge() -> impl IntoView {
    let notifications = expect_context::<RwSignal<Vec<ScanNotification>>>();
    let total = move || notifications.with(|v| v.iter().map(|n| n.count).sum::<usize>());

    view! {
        <Show when=move || ! notifications.with(|v| v.is_empty()) fallback=|| ()>
            <button
                class="scan-badge"
                title="New chapters found — click to dismiss"
                on:click=move |_| notifications.update(|v| v.clear())
            >
                "🔔 "
                <span class="scan-badge__count">{move || total()}</span>
                " new"
            </button>
        </Show>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let chapters = RwSignal::new(
        std::collections::HashMap::<i64, crate::types::ChapterProgress>::new(),
    );
    provide_context(chapters);

    let scan_notifications   = RwSignal::new(Vec::<ScanNotification>::new());
    provide_context(scan_notifications);

    let refresh_state        = RwSignal::new(crate::types::RefreshState::Idle);
    provide_context(refresh_state);

    let library_invalidation = RwSignal::new(0u32);
    provide_context(library_invalidation);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use crate::events::{DownloadProgressEvent, RefreshProgressEvent};
            use crate::types::LiveChapterStatus;
            use serde::Deserialize;
            use wasm_bindgen::prelude::*;
            use web_sys::{EventSource, MessageEvent};

            #[derive(Deserialize)]
            #[serde(tag = "type", rename_all = "snake_case")]
            enum IncomingEvent {
                StateSnapshot {
                    chapters: Vec<crate::types::ActiveDownloadState>,
                    #[serde(default)]
                    is_refreshing: bool,
                },
                NewChapters {
                    manga_id:   i64,
                    manga_name: String,
                    count:      usize,
                },
                #[serde(untagged)]
                Refresh(RefreshProgressEvent),
                #[serde(untagged)]
                Download(DownloadProgressEvent),
            }

            let es = match EventSource::new("/rest/events") {
                Ok(es) => es,
                Err(e) => {
                    log::error!("Failed to open EventSource: {:?}", e);
                    return;
                }
            };

            let es_for_close = es.clone();
            let on_close_signal =
                Closure::<dyn FnMut(MessageEvent)>::new(move |_: MessageEvent| {
                    es_for_close.close();
                });
            es.add_event_listener_with_callback(
                "close",
                on_close_signal.as_ref().unchecked_ref(),
            )
            .ok();
            on_close_signal.forget();

            let chapters_signal = chapters;
            let on_message =
                Closure::<dyn FnMut(MessageEvent)>::new(move |msg: MessageEvent| {
                    let data = match msg.data().as_string() {
                        Some(d) => d,
                        None => return,
                    };

                    let event: IncomingEvent = match serde_json::from_str(&data) {
                        Ok(e) => e,
                        Err(e) => {
                            log::warn!("Failed to parse SSE event: {e}");
                            return;
                        }
                    };

                    match event {
                        IncomingEvent::StateSnapshot { chapters, is_refreshing } => {
                            chapters_signal.update(|map| {
                                map.clear();
                                for chapter in chapters {
                                    let id = chapter.chapter_id;
                                    map.insert(id, chapter.into());
                                }
                            });
                            if is_refreshing {
                                refresh_state.set(crate::types::RefreshState::Running {
                                    completed: 0,
                                    total: 0,
                                });
                            }
                        }

                        IncomingEvent::NewChapters { manga_id, manga_name, count } => {
                            scan_notifications.update(|v| {
                                if let Some(n) =
                                    v.iter_mut().find(|n| n.manga_id == manga_id)
                                {
                                    n.count += count;
                                } else {
                                    v.push(ScanNotification {
                                        manga_id,
                                        manga_name,
                                        count,
                                    });
                                }
                            });
                        }

                        IncomingEvent::Refresh(event) => {
                            use crate::events::RefreshProgressEvent;
                            match event {
                                RefreshProgressEvent::Started { total } => {
                                    refresh_state.set(crate::types::RefreshState::Running {
                                        completed: 0,
                                        total,
                                    });
                                }
                                RefreshProgressEvent::MangaRefreshed { completed, total, .. } => {
                                    refresh_state.set(crate::types::RefreshState::Running { completed, total });
                                }
                                RefreshProgressEvent::Completed { total, failed } => {
                                    refresh_state.set(crate::types::RefreshState::Done { total, failed });
                                    library_invalidation.update(|n| *n += 1);
                                    set_timeout(
                                        move || refresh_state.set(crate::types::RefreshState::Idle),
                                        std::time::Duration::from_secs(5),
                                    );
                                }
                            }
                        }

                        IncomingEvent::Download(download_event) => {
                            let maybe_dismiss_id = match &download_event {
                                DownloadProgressEvent::ChapterCompleted {
                                    chapter_id, ..
                                } => Some(*chapter_id),
                                DownloadProgressEvent::ChapterFailed {
                                    chapter_id, ..
                                } => Some(*chapter_id),
                                DownloadProgressEvent::ChapterCancelled {
                                    chapter_id, ..
                                } => Some(*chapter_id),
                                _ => None,
                            };

                            chapters_signal.update(|map| match download_event {
                                DownloadProgressEvent::ChapterStarted {
                                    chapter_id,
                                    chapter_name,
                                    total_pages,
                                } => {
                                    map.entry(chapter_id)
                                        .and_modify(|c| {
                                            if total_pages > 0 {
                                                c.total_pages = total_pages;
                                            }
                                            if c.name.is_empty() {
                                                c.name = chapter_name.clone();
                                            }
                                        })
                                        .or_insert_with(|| crate::types::ChapterProgress {
                                            id: chapter_id,
                                            name: chapter_name,
                                            total_pages,
                                            completed_pages: 0,
                                            status: LiveChapterStatus::InProgress,
                                        });
                                }
                                DownloadProgressEvent::PageCompleted {
                                    chapter_id, ..
                                } => {
                                    if let Some(c) = map.get_mut(&chapter_id) {
                                        c.completed_pages += 1;
                                    }
                                }
                                DownloadProgressEvent::ChapterCompleted {
                                    chapter_id,
                                    successful_pages,
                                    ..
                                } => {
                                    if let Some(c) = map.get_mut(&chapter_id) {
                                        c.completed_pages = successful_pages;
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
                                    chapter_id, ..
                                } => {
                                    if let Some(c) = map.get_mut(&chapter_id) {
                                        c.status = LiveChapterStatus::Cancelled;
                                    }
                                }
                                DownloadProgressEvent::ChapterDeferred { chapter_id, reason, .. } => {
                                    if let Some(c) = map.get_mut(&chapter_id) {
                                        c.status = LiveChapterStatus::Failed(reason);
                                    }
                                }
                            });

                            if let Some(id) = maybe_dismiss_id {
                                let s = chapters_signal;
                                set_timeout(
                                    move || {
                                        s.update(|m| {
                                            if let Some(c) = m.get_mut(&id) {
                                                if matches!(
                                                    c.status,
                                                    LiveChapterStatus::Completed
                                                ) {
                                                    c.status =
                                                        LiveChapterStatus::CompletedHidden;
                                                } else {
                                                    m.remove(&id);
                                                }
                                            }
                                        });
                                    },
                                    std::time::Duration::from_secs(5),
                                );
                            }
                        }
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

    view! {
        <Stylesheet id="leptos" href="/pkg/kani-web.css"/>
        <Title text="Kani Manga Reader"/>
        <div id="root">
            <Router>
                <header>
                    <A href="/">"Kani"</A>
                    <A href="/sources">"Sources"</A>
                    <A href="/search">"Search"</A>
                    <A href="/settings">"Settings"</A>
                    <A href="/updates">"Recent Updates"</A>
                    <ScanBadge/>
                </header>
                <main class="container">
                    <Routes fallback=|| view! { <h1>"Not Found"</h1> }>
                        <Route path=path!("/") view=Library/>
                        <Route path=path!("/sources") view=Sources/>
                        <Route path=path!("/source/:id") view=SourceDetails/>
                        <Route path=path!("/source/:id/manga/:manga_id") view=MangaDetails/>
                        <Route path=path!("/manga/:db_id") view=MangaDetails/>
                        <Route path=path!("/search") view=GlobalSearch/>
                        <Route path=path!("/settings") view=Settings/>
                        <Route path=path!("/updates") view=RecentUpdates/>
                    </Routes>
                </main>
            </Router>
            <DownloadProgress/>
        </div>
    }
}