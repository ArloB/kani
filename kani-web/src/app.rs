use crate::pages::downloads::DownloadProgress;
use crate::pages::settings::Settings;
use crate::pages::sources::Sources;
use crate::pages::library::Library;
use crate::pages::manga_details::MangaDetails;
use crate::pages::source_details::SourceDetails;
use crate::pages::global_search::GlobalSearch;
use crate::pages::recent_updates::RecentUpdates;
use crate::pages::login::Login;
use crate::pages::components::permission_handlers::PermissionGate;
use crate::server_fns::get_my_permissions;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::hooks::use_location;
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
fn Nav() -> impl IntoView {
    let location = use_location();
    let is_login = move || location.pathname.get() == "/login";

    view! {
        <Show when=move || !is_login() fallback=|| ()>
            <header>
                <A href="/">"Kani"</A>
                <PermissionGate permission="source:browse">
                    <A href="/sources">"Sources"</A>
                </PermissionGate>
                <PermissionGate permission="source:browse">
                    <A href="/search">"Search"</A>
                </PermissionGate>
                <PermissionGate permission="settings:view">
                    <A href="/settings">"Settings"</A>
                </PermissionGate>
                <PermissionGate permission="library:view">
                    <A href="/updates">"Recent Updates"</A>
                </PermissionGate>
                <ScanBadge/>
                <form
                    method="post"
                    action="/rest/auth/logout"
                    class="logout-form"
                >
                    <button type="submit" class="logout-btn" title="Sign out">
                        "Sign out"
                    </button>
                </form>
            </header>
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

    let permissions = Resource::new(|| (), |_| get_my_permissions());
    provide_context(permissions);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use crate::events::{DownloadProgressEvent, RefreshProgressEvent};
            use crate::types::LiveChapterStatus;
            use serde::Deserialize;
            use std::cell::{Cell, RefCell};
            use std::rc::Rc;
            use wasm_bindgen::prelude::*;
            use web_sys::MessageEvent;

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

            let retry_count: Rc<Cell<u32>> = Rc::new(Cell::new(0));
            // Tracks the active EventSource so reconnects can close the old one first.
            let es_holder: Rc<RefCell<Option<web_sys::EventSource>>> =
                Rc::new(RefCell::new(None));

            // Late-binding holder for the connect fn, allowing the onerror handler
            // to schedule a reconnect via a Weak reference (avoids Rc cycle).
            let connect_holder: Rc<RefCell<Option<Box<dyn Fn()>>>> =
                Rc::new(RefCell::new(None));

            let retry_for_setup = retry_count.clone();
            let connect_weak    = Rc::downgrade(&connect_holder);
            let chapters_signal = chapters;

            let actual_connect = move || {
                // Close any existing connection before opening a new one
                if let Some(old) = es_holder.borrow_mut().take() {
                    old.close();
                }

                let es = match web_sys::EventSource::new("/rest/events") {
                    Ok(es) => es,
                    Err(e) => {
                        log::error!("Failed to open EventSource: {:?}", e);
                        return;
                    }
                };

                *es_holder.borrow_mut() = Some(es.clone());

                // onopen: reset retry counter on successful connection
                let retry_open = retry_for_setup.clone();
                let onopen = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
                    retry_open.set(0);
                });
                es.set_onopen(Some(onopen.as_ref().unchecked_ref()));
                onopen.forget();

                // onerror: schedule reconnect with exponential backoff
                let retry_err    = retry_for_setup.clone();
                let connect_err  = connect_weak.clone();
                let onerror = Closure::<dyn FnMut(_)>::new(move |_: web_sys::Event| {
                    let count     = retry_err.get();
                    let delay_ms  = (1000u32 * 2u32.pow(count.min(4))).min(30_000);
                    retry_err.set(count + 1);
                    log::info!("SSE disconnected, reconnecting in {}ms (attempt {})", delay_ms, count + 1);
                    let connect_retry = connect_err.clone();
                    gloo_timers::callback::Timeout::new(delay_ms, move || {
                        if let Some(rc) = connect_retry.upgrade() {
                            if let Some(f) = rc.borrow().as_ref() {
                                f();
                            }
                        }
                    })
                    .forget();
                });
                es.set_onerror(Some(onerror.as_ref().unchecked_ref()));
                onerror.forget();

                // close event (app-level close signal)
                let es_for_close = es.clone();
                let on_close = Closure::<dyn FnMut(MessageEvent)>::new(
                    move |_: MessageEvent| { es_for_close.close(); },
                );
                es.add_event_listener_with_callback(
                    "close",
                    on_close.as_ref().unchecked_ref(),
                )
                .ok();
                on_close.forget();

                // message handler
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
                                    RefreshProgressEvent::MangaRefreshed {
                                        completed, total, ..
                                    } => {
                                        refresh_state.set(crate::types::RefreshState::Running {
                                            completed,
                                            total,
                                        });
                                    }
                                    RefreshProgressEvent::Completed { total, failed } => {
                                        refresh_state
                                            .set(crate::types::RefreshState::Done { total, failed });
                                        library_invalidation.update(|n| *n += 1);
                                        set_timeout(
                                            move || {
                                                refresh_state
                                                    .set(crate::types::RefreshState::Idle)
                                            },
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
                                    DownloadProgressEvent::ChapterDeferred {
                                        chapter_id,
                                        reason,
                                        ..
                                    } => {
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
            };

            *connect_holder.borrow_mut() = Some(Box::new(actual_connect));

            // Initial connection
            if let Some(f) = connect_holder.borrow().as_ref() {
                f();
            }

            // App is the root component and lives for the entire browser session.
            // Keep connect_holder alive so that Weak refs inside onerror handlers
            // can upgrade and trigger reconnects. Intentional leak.
            std::mem::forget(connect_holder);
        }
    });

    view! {
        <Stylesheet id="leptos" href="/pkg/kani-web.css"/>
        <Title text="Kani Manga Reader"/>
        <div id="root">
            <Router>
                <Nav/>
                <main class="container">
                    <Routes fallback=|| view! { <h1>"Not Found"</h1> }>
                        <Route path=path!("/login") view=Login/>
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
