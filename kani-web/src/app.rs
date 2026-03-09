use crate::pages::downloads::DownloadProgress;
use crate::pages::home::Home;
use crate::pages::library::Library;
use crate::pages::manga_details::MangaDetails;
use crate::pages::source_details::SourceDetails;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    let chapters = RwSignal::new(std::collections::HashMap::<i64, crate::types::ChapterProgress>::new());
    provide_context(chapters);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use crate::events::DownloadProgressEvent;
            use crate::types::LiveChapterStatus;
            use wasm_bindgen::prelude::*;
            use web_sys::{EventSource, MessageEvent};

            let es = match EventSource::new("/rest/downloads/progress") {
                Ok(es) => es,
                Err(e) => {
                    log::error!("Failed to open EventSource: {:?}", e);
                    return;
                }
            };

            let es_for_close = es.clone();
            let on_close_signal = Closure::<dyn FnMut(MessageEvent)>::new(move |_: MessageEvent| {
                es_for_close.close();
            });
            es.add_event_listener_with_callback("close", on_close_signal.as_ref().unchecked_ref()).ok();
            on_close_signal.forget();

            let chapters_signal = chapters;
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |msg: MessageEvent| {
                let data = match msg.data().as_string() {
                    Some(d) => d,
                    None => return,
                };

                if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&data)
                    && raw["type"] == "state_snapshot" {
                         if let Ok(chapters) = serde_json::from_value::<Vec<crate::types::ActiveDownloadState>>(
                            raw["chapters"].clone()
                        ) {
                            chapters_signal.update(|map| {
                                map.clear();
                                for chapter in chapters {
                                    let id = chapter.chapter_id;
                                    map.insert(id, chapter.into());
                                }
                            });
                        }

                        return;
                    }

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
                    DownloadProgressEvent::PageCompleted { chapter_id, .. } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.completed_pages += 1;
                        }
                    }
                    DownloadProgressEvent::ChapterCompleted {
                        chapter_id,
                        successful_pages,                        ..
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
                                if let Some(c) = m.get_mut(&id) {
                                    if matches!(c.status, LiveChapterStatus::Completed) {
                                        c.status = LiveChapterStatus::CompletedHidden;
                                    } else {
                                        m.remove(&id);
                                    }
                                }
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

    view! {
        <Stylesheet id="leptos" href="/pkg/kani-web.css"/>
        <Title text="Kani Manga Reader"/>
        <div id="root">
            <Router>
                <header>
                    <A href="/">"Kani"</A>
                </header>
                <main class="container">
                    <Routes fallback=|| view! { <h1>"Not Found"</h1> }>
                        <Route path=path!("/") view=Home/>
                        <Route path=path!("/source/:id") view=SourceDetails/>
                        <Route path=path!("/source/:id/manga/:manga_id") view=MangaDetails/>
                        <Route path=path!("/manga/:db_id") view=MangaDetails/>
                        <Route path=path!("/library") view=Library/>
                    </Routes>
                </main>
            </Router>
            <DownloadProgress/>
        </div>
    }
}
