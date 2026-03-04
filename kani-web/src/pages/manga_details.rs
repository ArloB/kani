use crate::server_fns::{fetch_sources, get_chapter_list, get_manga_details, proxy_url, save_to_library};
use crate::types::ChapterList;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

#[allow(non_snake_case)]
#[component]
pub fn MangaDetails() -> impl IntoView {
    let params = use_params_map();
    let source_id = move || {
        params.with(|params| {
            params
                .get("id")
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or(0)
        })
    };
    let manga_id = move || params.with(|params| params.get("manga_id").unwrap_or_default());

    let (page, set_page) = signal(1);

    let manga = Resource::new(
        move || (source_id(), manga_id()),
        move |(sid, mid)| async move {
            let manga = get_manga_details(sid, mid.clone()).await;
            let sources = fetch_sources().await.unwrap_or_default();
            let source = sources.into_iter().find(|s| s.id == sid);

            match (manga, source) {
                (Ok(m), Some(s)) => Ok((m, s)),
                (Err(e), _) => Err(e),
                (_, None) => Err(leptos::server_fn::error::ServerFnError::new(
                    "Source not found",
                )),
            }
        },
    );

    let chapters = Resource::new(
        move || (source_id(), manga_id(), page.get()),
        move |(sid, mid, p)| async move { get_chapter_list(sid, mid, p).await },
    );

    let (library_pending, set_library_pending) = signal(false);

    view! {
        <div class="manga-details">
            <Suspense fallback=move || view! { <p>"Loading details..."</p> }>
                {move || {
                    manga.get().map(|res| match res {
                        Ok((info, source)) => {
                            let sid = source_id();
                            let mid = manga_id();
                            let base_url = source.base_url.clone();
                            let info_title = info.title.clone();
                            let info_cover = info.cover_url.clone();
                            let info_status = info.status.to_string();
                            let info_authors = info.authors.clone();
                            let info_artists = info.artists.clone();
                            let info_tags = info.tags.clone();
                            let info_description = info.description.clone();
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
                                            <button
                                                class="add-to-library"
                                                disabled=library_pending
                                                on:click=move |_| {
                                                    let m = mid.clone();
                                                    set_library_pending.set(true);
                                                    leptos::task::spawn_local(async move {
                                                        let _ = save_to_library(sid, m).await;
                                                        set_library_pending.set(false);
                                                    });
                                                }
                                            >
                                                {move || if library_pending.get() { "Saving..." } else { "Add to Library" }}
                                            </button>
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
                                                        <button class="download-button">
                                                            "Download"
                                                        </button>
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
