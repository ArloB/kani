use crate::pages::components::cover_image::CoverImage;
use crate::pages::components::pagination::Pagination;
use crate::server_fns::{get_popular_manga, get_source, search_manga};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

fn skeleton_source_grid() -> impl IntoView {
    view! {
        <div class="skeleton-manga-grid">
            {(0..20).map(|_| view! {
                <div class="skeleton-card">
                    <div class="skeleton-card__cover"></div>
                    <div class="skeleton-card__title skeleton-card__title--long"></div>
                    <div class="skeleton-card__title skeleton-card__title--short"></div>
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

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

    let (page, set_page)               = signal(1i32);
    let (input_value, set_input_value) = signal(String::new());

    // Debounce the raw input — the resource key tracks this directly so no
    // intermediate `query` signal or Effect needed.
    let debounced_query = crate::utils::use_debounced_signal(input_value, 600);

    // Reset to page 1 whenever the search query changes (but not on mount).
    Effect::new(move |prev: Option<String>| {
        let q = debounced_query.get();
        if prev.is_some() {
            set_page.set(1);
        }
        q
    });

    let source = Resource::new(id, move |sid| async move { get_source(sid).await });

    let manga_list = Resource::new(
        move || (id(), page.get(), debounced_query.get()),
        move |(sid, p, q)| async move {
            if q.is_empty() {
                get_popular_manga(sid, p).await
            } else {
                search_manga(sid, q, p).await
            }
        },
    );

    let (is_pending, set_pending) = signal(false);

    view! {
        <div class="source-details">
            <header class="sticky-header">
                <h2>
                    <Suspense fallback=move || view! { "Source " {id} }>
                        {move || source.get().map(|res| match res {
                            Ok(s)  => s.name.clone().into_any(),
                            Err(_) => view! { "Source " {id} }.into_any(),
                        })}
                    </Suspense>
                </h2>
                <div class="search-row">
                    <input
                        type="text"
                        placeholder="Search..."
                        prop:value=input_value
                        on:input=move |ev| set_input_value.set(event_target_value(&ev))
                    />
                </div>
            </header>

            <div class="page-loading-bar" class:page-loading-bar--active=move || is_pending.get()>
                <div class="page-loading-bar__fill"></div>
            </div>

            <Transition fallback=skeleton_source_grid set_pending>
                {move || manga_list.get().map(|res| match res {
                    Err(e) => view! {
                        <p class="error">"Error: " {e.to_string()}</p>
                    }.into_any(),
                    Ok(list) if list.manga.is_empty() => view! {
                        <p class="empty">"No results found."</p>
                    }.into_any(),
                    Ok(list) => {
                        let has_next = list.has_next_page;
                        view! {
                            <div
                                class="manga-grid"
                                class:content--stale=move || is_pending.get()
                            >
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
                            <Pagination
                                page
                                set_page
                                has_next=Signal::derive(move || has_next)
                            />
                        }.into_any()
                    }
                })}
            </Transition>
        </div>
    }
}