use crate::server_fns::{
    cancel_download, check_in_library, delete_downloaded, delete_manga, download_all, fetch_sources, get_chapter_list, get_local_chapter_list,
    get_local_manga, get_manga_details, proxy_url, save_to_library, start_download,
};
use crate::types::{ChapterList, LiveChapterStatus};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

#[derive(Debug, Clone, PartialEq)]
struct LiveProgress {
    pub total_pages: usize,
    pub completed_pages: usize,
    pub status: LiveChapterStatus,
}

#[allow(non_snake_case)]
#[component]
pub fn MangaDetails() -> impl IntoView {
    let params = use_params_map();
    let source_id = move || params.with(|p| p.get("id").unwrap_or_default().parse::<i64>().ok());
    let manga_id = move || params.with(|p| p.get("manga_id"));
    let db_id = move || params.with(|p| p.get("db_id").unwrap_or_default().parse::<i64>().ok());

    let (page, set_page) = signal(1);
    let (added_db_id, set_added_db_id) = signal::<Option<i64>>(None);
    let live_progress = RwSignal::new(std::collections::HashMap::<i64, LiveProgress>::new());
    
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

            let prog_sig = live_progress;
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |msg: MessageEvent| {
                let data = match msg.data().as_string() {
                    Some(d) => d,
                    None => return,
                };
                let event: DownloadProgressEvent = match serde_json::from_str(&data) {
                    Ok(e) => e,
                    Err(_) => return,
                };

                prog_sig.update(|map| match event {
                    DownloadProgressEvent::ChapterStarted { chapter_id, total_pages, .. } => {
                        map.insert(chapter_id, LiveProgress {
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
                    DownloadProgressEvent::ChapterCompleted { chapter_id, successful_pages, .. } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.completed_pages = successful_pages;
                            c.status = LiveChapterStatus::Completed;
                        } else {
                            map.insert(chapter_id, LiveProgress {
                                total_pages: successful_pages,
                                completed_pages: successful_pages,
                                status: LiveChapterStatus::Completed,
                            });
                        }
                    }
                    DownloadProgressEvent::ChapterFailed { chapter_id, error, .. } => {
                        if let Some(c) = map.get_mut(&chapter_id) {
                            c.status = LiveChapterStatus::Failed(error.clone());
                        } else {
                            map.insert(chapter_id, LiveProgress {
                                total_pages: 0,
                                completed_pages: 0,
                                status: LiveChapterStatus::Failed(error.clone()),
                            });
                        }
                        
                        let _ = leptos::task::spawn_local(async move {
                            // give the UI a moment to show the failed state
                            use leptos::prelude::set_timeout;
                            use std::time::Duration;
                            set_timeout(move || {
                                prog_sig.update(|m| { m.remove(&chapter_id); });
                            }, Duration::from_millis(3000));
                        });
                    }
                    DownloadProgressEvent::ChapterCancelled { chapter_id, .. } => {
                        map.remove(&chapter_id);
                    }
                });
            });
            es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
            
            let es_clone = es.clone();
            on_cleanup(move || { es_clone.close(); });
        }
    });
    
    let is_local = move || db_id().is_some();

    let manga = Resource::new(
        move || (source_id(), manga_id(), db_id()),
        move |(sid, mid, did)| async move {
            if let Some(did) = did {
                let (info, source) = get_local_manga(did).await?;
                Ok((info, source, Some(did), true))
            } else if let (Some(sid), Some(mid)) = (sid, mid) {
                let info = get_manga_details(sid, mid.clone()).await?;
                let sources = fetch_sources().await.unwrap_or_default();
                let source = sources.into_iter().find(|s| s.id == sid)
                    .ok_or_else(|| ServerFnError::new("Source not found"))?;
                let existing_db_id = check_in_library(sid, mid).await?;
                Ok((info, source, existing_db_id, false))
            } else {
                Err(ServerFnError::new("Invalid route parameters"))
            }
        },
    );

    let chapters = Resource::new(
        move || (source_id(), manga_id(), db_id(), page.get()),
        move |(sid, mid, did, p)| async move {
            if let Some(did) = did {
                get_local_chapter_list(did, p).await
            } else if let (Some(sid), Some(mid)) = (sid, mid) {
                get_chapter_list(sid, mid, p).await
            } else {
                Err(ServerFnError::new("Invalid route parameters"))
            }
        },
    );

    let (library_pending, set_library_pending) = signal(false);

    view! {
        <div class="manga-details">
            <Suspense fallback=move || view! { <p>"Loading details..."</p> }>
                {move || {
                    manga.get().map(|res| match res {
                        Ok((info, source, initial_db_id, is_local_route)) => {
                            let current_db_id = move || added_db_id.get().or(initial_db_id);
                            let sid = source.id;
                            let mid = info.id.clone();
                            let base_url = source.base_url.clone();
                            let info_title = info.title.clone();
                            let info_cover = info.cover_url.clone();
                            let info_status = info.status.to_string();
                            let info_authors = info.authors.clone();
                            let info_artists = info.artists.clone();
                            let info_tags = info.tags.clone();
                            let info_description = info.description.clone();
                            
                            let did_val = db_id().unwrap_or_default();

                            view! {
                                <div class="details-header">
                                    <h1>{info.title.clone()}</h1>
                                    <div class="cover">
                                        {match info_cover {
                                            Some(url) => {
                                                let src = proxy_url(&url, &base_url);
                                                view! { <img src=src alt=info_title style="max-width: 300px" /> }.into_any()
                                            },
                                            None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                                        }}
                                    </div>
                                    <div class="info">
                                        <div class="details">
                                            <div class="status">
                                                <p>"Status: " {info_status}</p>
                                            </div>
                                            <div class="people">
                                                <div class="authors">
                                                    <p>"Author: "</p>
                                                    <For
                                                        each=move || info_authors.clone()
                                                        key=|author: &String| author.clone()
                                                        children=move |author: String| view! {
                                                            <div class="author">
                                                                <A href=format!("/search?author={}", author)>{author}</A>
                                                            </div>
                                                        }
                                                    />
                                                </div>
                                                <div class="artists">
                                                    <p>"Artist: "</p>
                                                    <For
                                                        each=move || info_artists.clone()
                                                        key=|artist: &String| artist.clone()
                                                        children=move |artist: String| view! {
                                                            <div class="artist">
                                                                <A href=format!("/search?artist={}", artist)>{artist}</A>
                                                            </div>
                                                        }
                                                    />
                                                </div>
                                            </div>
                                            <div class="tags">
                                                <For
                                                    each=move || info_tags.clone()
                                                    key=|tag: &String| tag.clone()
                                                    children=move |tag: String| view! {
                                                        <div class="tag">
                                                            <A href=format!("/search?tag={}", tag)>{tag}</A>
                                                        </div>
                                                    }
                                                />
                                            </div>
                                            
                                            <div class="library-actions">
                                                {move || {
                                                    if is_local_route {
                                                        view! {
                                                            <button class="migrate-button">"Migrate"</button>
                                                            <button class="remove-button" on:click=move |_| {
                                                                leptos::task::spawn_local(async move {
                                                                    if delete_manga(did_val).await.is_ok() {
                                                                        let navigate = leptos_router::hooks::use_navigate();
                                                                        navigate("/", Default::default());
                                                                    }
                                                                });
                                                            }>"Remove from library"</button>
                                                            <button class="download-all-button" on:click=move |_| {
                                                                leptos::task::spawn_local(async move {
                                                                    let _ = download_all(did_val).await;
                                                                });
                                                            }>"Download All"</button>
                                                        }.into_any()
                                                    } else if let Some(id) = current_db_id() {
                                                        view! {
                                                            <A href=format!("/manga/{}", id)>
                                                                <button class="go-to-library-button">"Go to Library"</button>
                                                            </A>
                                                        }.into_any()
                                                    } else {
                                                        let m_clone = mid.clone();
                                                        view! {
                                                            <button
                                                                class="add-to-library"
                                                                disabled=library_pending
                                                                on:click=move |_| {
                                                                    let m = m_clone.clone();
                                                                    set_library_pending.set(true);
                                                                    leptos::task::spawn_local(async move {
                                                                        if let Ok(new_id) = save_to_library(sid, m).await {
                                                                            set_added_db_id.set(Some(new_id));
                                                                        }
                                                                        set_library_pending.set(false);
                                                                    });
                                                                }
                                                            >
                                                                {move || if library_pending.get() { "Saving..." } else { "Add to Library" }}
                                                            </button>
                                                        }.into_any()
                                                    }
                                                }}
                                            </div>

                                            <div class="description">
                                                <p>{info_description}</p>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        },
                        Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>

            <Suspense fallback=move || view! { <p>"Loading chapters..."</p> }>
                {move || {
                    chapters.get().map(|res: Result<ChapterList, ServerFnError>| match res {
                        Ok(list) => {
                            let list_chapters = list.clone();
                            let list_pagination = list.clone();
                            view! {
                            <div class="chapter-list-group">
                                <h2>"Chapters"</h2>
                                <div class="chapter-list">
                                    <For
                                        each=move || list_chapters.chapters.clone()
                                        key=|chapter| chapter.id.clone()
                                        children=move |chapter| {
                                            let chap_id_str = chapter.id.clone();
                                            view! {
                                                <div class="chapter-item">
                                                    <div class="chapter-details">
                                                        <span class="chapter-title">
                                                            {{
                                                                let mut title_str = String::new();
                                                                if let Some(vol) = &chapter.volume {
                                                                    title_str.push_str(&format!("Vol. {} ", vol));
                                                                }
                                                                title_str.push_str(&format!("Ch. {}", chapter.number));
                                                                if let Some(title) = &chapter.title
                                                                    && !title.is_empty() {
                                                                        title_str.push_str(&format!(" - {}", title));
                                                                    }
                                                                title_str
                                                            }}
                                                        </span>
                                                        <span class="chapter-scanlator">{chapter.scanlator.unwrap_or_default()}</span>
                                                        <span class="chapter-date">{
                                                            chapter.date_uploaded.map(|epoch| {
                                                                use chrono::DateTime;
                                                                DateTime::from_timestamp(epoch, 0)
                                                                    .map(|dt| dt.format("%b %d, %Y").to_string())
                                                                    .unwrap_or_default()
                                                            }).unwrap_or_default()
                                                        }</span>
                                                    </div>
                                                    
                                                    <div class="chapter-actions">
                                                        <Show when=move || is_local() fallback=|| ()>
                                                            {
                                                                let cid = chap_id_str.clone();
                                                                let db_chap_id = cid.parse::<i64>().unwrap_or(0);
                                                                let is_downloaded = chapter.download_status == 2;
                                                                let is_downloading_db = chapter.download_status == 1;

                                                                view! {
                                                                    {move || {
                                                                        let map = live_progress.get();
                                                                        let live = map.get(&db_chap_id);

                                                                        let status = match live {
                                                                            Some(p) => match &p.status {
                                                                                LiveChapterStatus::InProgress => 1,
                                                                                LiveChapterStatus::Completed => 2,
                                                                                LiveChapterStatus::Failed(_) => 3,
                                                                                LiveChapterStatus::Cancelled | LiveChapterStatus::Deleted => 0,
                                                                            },
                                                                            None => if is_downloaded { 2 } else if is_downloading_db { 1 } else { 0 },
                                                                        };

                                                                        match status {
                                                                            2 => view! {
                                                                                <button class="delete-button" style="background-color: var(--color-error); color: white;" on:click=move |_| {
                                                                                    leptos::task::spawn_local(async move {
                                                                                        if delete_downloaded(db_chap_id).await.is_ok() {
                                                                                            live_progress.update(|m| {
                                                                                                m.insert(db_chap_id, LiveProgress {
                                                                                                    total_pages: 0,
                                                                                                    completed_pages: 0,
                                                                                                    status: LiveChapterStatus::Deleted,
                                                                                                });
                                                                                            });
                                                                                        }
                                                                                    });
                                                                                }>
                                                                                    "Delete"
                                                                                </button>
                                                                            }.into_any(),
                                                                            1 => {
                                                                                let text = if let Some(p) = live {
                                                                                    if p.total_pages > 0 {
                                                                                        format!("Downloading... ({}/{})", p.completed_pages, p.total_pages)
                                                                                    } else {
                                                                                        "Downloading...".to_string()
                                                                                    }
                                                                                } else {
                                                                                    "Downloading...".to_string()
                                                                                };
                                                                                view! {
                                                                                    <button class="download-button" style="background-color: var(--color-accent); color: var(--color-bg);" on:click=move |_| {
                                                                                        leptos::task::spawn_local(async move {
                                                                                            // Optimistically clear the UI
                                                                                            live_progress.update(|m| { m.remove(&db_chap_id); });
                                                                                            let _ = cancel_download(db_chap_id).await;
                                                                                        });
                                                                                    }>
                                                                                        {text}
                                                                                    </button>
                                                                                }.into_any()
                                                                            },
                                                                            3 => {
                                                                                let msg = if let Some(LiveProgress { status: LiveChapterStatus::Failed(err), .. }) = live {
                                                                                    format!("Failed: {}", err)
                                                                                } else {
                                                                                    "Failed".to_string()
                                                                                };
                                                                                view! {
                                                                                    <button class="download-button" disabled=true style="background-color: var(--color-error); color: white;">
                                                                                        {msg}
                                                                                    </button>
                                                                                }.into_any()
                                                                            },
                                                                            _ => view! {
                                                                                <button class="download-button" on:click=move |_| {
                                                                                    leptos::task::spawn_local(async move {
                                                                                        live_progress.update(|m| {
                                                                                            m.insert(db_chap_id, LiveProgress {
                                                                                                total_pages: 0,
                                                                                                completed_pages: 0,
                                                                                                status: LiveChapterStatus::InProgress,
                                                                                            });
                                                                                        });
                                                                                        let _ = start_download(db_chap_id).await;
                                                                                    });
                                                                                }>
                                                                                    "Download"
                                                                                </button>
                                                                            }.into_any()
                                                                        }
                                                                    }}
                                                                }
                                                            }
                                                        </Show>
                                                    </div>
                                                </div>
                                            }
                                        }
                                    />
                                    <Show when=move || !list_pagination.chapters.is_empty() fallback=move || view! { <p>"No chapters found."</p> }>
                                        <div class="pagination">
                                            <button on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1)) disabled=move || page.get() <= 1>"Prev"</button>
                                            <span>" Page " {page} </span>
                                            <button on:click=move |_| set_page.update(|p| *p += 1) disabled=move || !list.has_next_page>"Next"</button>
                                        </div>
                                    </Show>
                                </div>
                            </div>
                        }.into_any()
                    },
                        Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}