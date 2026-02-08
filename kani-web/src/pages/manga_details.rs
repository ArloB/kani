use crate::api::{get_chapter_list, get_manga_details};
use leptos::prelude::*;
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

    let manga = LocalResource::new(move || {
        let sid = source_id();
        let mid = manga_id();
        async move {
            let manga = get_manga_details(sid, &mid).await;
            let sources = crate::api::fetch_sources().await.unwrap_or_default();
            let source = sources.iter().find(|s| s.id == sid).cloned();

            match (manga, source) {
                (Ok(m), Some(s)) => Ok((m, s)),
                (Err(e), _) => Err(e),
                (_, None) => Err(crate::api::ApiError::NotFound(
                    "Source not found".to_string(),
                )),
            }
        }
    });

    let chapters = LocalResource::new(move || {
        let sid = source_id();
        let mid = manga_id();
        async move { get_chapter_list(sid, &mid, 1).await } // TODO: Pagination for chapters?
    });

    view! {
        <div class="manga-details">
            <Suspense fallback=move || view! { <p>"Loading details..."</p> }>
                {move || {
                    manga.get().map(|res| match res {
                        Ok((info, source)) => view! {
                            <div class="details-header">
                                <div class="cover">
                                    {match info.cover_url {
                                        Some(url) => view! { <img src=crate::api::proxy_url(&url, &source.base_url) alt=info.title.clone() style="max-width: 300px" /> }.into_any(),
                                        None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                                    }}
                                </div>
                                <div class="info">
                                    <h1>{info.title}</h1>
                                    <p class="author">"Author: " {info.authors.join(", ")}</p>
                                    <p class="artist">"Artist: " {info.artists.join(", ")}</p>
                                    <div class="tags">
                                        <For
                                            each=move || info.tags.clone()
                                            key=|tag| tag.clone()
                                            children=move |tag| view! { <span class="tag">{tag}</span> }
                                        />
                                    </div>
                                    <div class="description">
                                        <p>{info.description}</p>
                                    </div>
                                </div>
                            </div>
                        }.into_any(),
                        Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>

            <Suspense fallback=move || view! { <p>"Loading chapters..."</p> }>
                {move || {
                    chapters.get().map(|res| match res {
                        Ok(list) => view! {
                            <div class="chapter-list">
                                <h2>"Chapters"</h2>
                                <For
                                    each=move || list.chapters.clone()
                                    key=|chapter| chapter.id.clone()
                                    children=move |chapter| {
                                        view! {
                                            <div class="chapter-item">
                                                 // Link removed as Reader is not implemented
                                                <span class="chapter-title">{chapter.title}</span>
                                                <span class="chapter-date">{chapter.date_uploaded.unwrap_or_default()}</span>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        }.into_any(),
                        Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}
