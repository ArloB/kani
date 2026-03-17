use crate::pages::components::cover_image::CoverImage;
use crate::pages::components::pagination::Pagination;
use crate::server_fns::{get_popular_manga, get_source, search_manga};
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

    let source = Resource::new(id, move |sid| async move { get_source(sid).await });

    let manga_list = Resource::new(
        move || (id(), page.get(), query.get()),
        move |(sid, p, q)| async move {
            if q.is_empty() {
                get_popular_manga(sid, p).await
            } else {
                search_manga(sid, q, p).await
            }
        },
    );

    let (input_value, set_input_value) = signal("".to_string());

    view! {
        <div class="source-details">
            <header class="sticky-header">
                <h2>
                    <Suspense fallback=move || view! { "Source " {id} }>
                        {move || source.get().map(|res| match res {
                            Ok(s) => s.name.clone().into_any(),
                            Err(_) => view! { "Source " {id} }.into_any(),
                        })}
                    </Suspense>
                </h2>
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
                        Ok(list) => {
                            view! {
                            <div class="manga-grid">
                                <For
                                    each=move || list.manga.clone()
                                    key=|manga| manga.id.clone()
                                    children=move |manga| {
                                        view! {
                                            <div class="manga-card">
                                                <A href=format!("/source/{}/manga/{}", id(), manga.id)>
                                                    <CoverImage url=manga.cover_url alt=manga.title.clone() />
                                                    <h3>{manga.title}</h3>
                                                </A>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                            <Pagination page set_page has_next=Signal::derive(move || list.has_next_page) />
                        }.into_any()
                        },
                        Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}
