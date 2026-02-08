use crate::api::{get_popular_manga, search_manga};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

#[component]
pub fn SourceDetails() -> impl IntoView {
    let params = use_params_map();
    let id = move || {
        params.with(|params| {
            params
                .get("id")
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or(0)
        })
    };

    let (page, set_page) = signal(1);
    let (query, set_query) = signal("".to_string());

    let manga_list = LocalResource::new(move || {
        let sid = id();
        let p = page.get();
        let q = query.get();
        async move {
            let sources = crate::api::fetch_sources().await.unwrap_or_default();
            let source = sources.iter().find(|s| s.id == sid).cloned();

            let list = if q.is_empty() {
                get_popular_manga(sid, p).await
            } else {
                search_manga(sid, &q, p).await
            };

            match (list, source) {
                (Ok(l), Some(s)) => Ok((l, s)),
                (Err(e), _) => Err(e),
                (_, None) => Err(crate::api::ApiError::NotFound(
                    "Source not found".to_string(),
                )),
            }
        }
    });

    let (input_value, set_input_value) = signal("".to_string());

    view! {
        <div class="source-details">
            <header class="sticky-header">
                <h2>"Source " {id}</h2>
                <form on:submit=move |ev| { ev.prevent_default(); set_query.set(input_value.get()); set_page.set(1); }>
                    <input
                        type="text"
                        placeholder="Search..."
                        on:input=move |ev| set_input_value.set(event_target_value(&ev))
                        prop:value=input_value
                    />
                    <button type="submit">"Search"</button>
                </form>
            </header>

            <Suspense fallback=move || view! { <p>"Loading manga..."</p> }>
                {move || {
                    manga_list.get().map(|res| match res {
                        Ok((list, source)) => {
                            let base_url = source.base_url.clone();
                            view! {
                            <div class="manga-grid">
                                <For
                                    each=move || list.manga.clone()
                                    key=|manga| manga.id.clone()
                                    children=move |manga| {
                                        let base_url = base_url.clone();
                                        view! {
                                            <div class="manga-card">
                                                <A href=format!("/source/{}/manga/{}", id(), manga.id)>
                                                    <div class="cover-image">
                                                        {match manga.cover_url {
                                                            Some(url) => view! { <img src=crate::api::proxy_url(&url, &base_url) alt=manga.title.clone() /> }.into_any(),
                                                            None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                                                        }}
                                                    </div>
                                                    <h3>{manga.title}</h3>
                                                </A>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                            <div class="pagination">
                                <button on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1)) disabled=move || page.get() <= 1>"Prev"</button>
                                <span>" Page " {page} </span>
                                <button on:click=move |_| set_page.update(|p| *p += 1) disabled=move || !list.has_next_page>"Next"</button>
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
