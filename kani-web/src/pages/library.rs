use crate::{
    pages::components::{cover_image::CoverImage, pagination::Pagination, combobox::Combobox},
    server_fns::{
        get_all_artists, get_all_authors, get_all_tags, get_categories,
        get_library, start_refresh_all,
    },
    types::{Category, MangaSortOrder, RefreshState},
};
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};

#[cfg(feature = "ssr")]
use leptos::server_fn::error::ServerFnError;

fn sync_filter_from_url(
    resource: Resource<Result<Vec<(i64, String)>, ServerFnError>>,
    url_getter: impl Fn() -> Option<String> + 'static,
    setter: WriteSignal<Option<i64>>,
) {
    Effect::new(move |_| {
        if let Some(Ok(items)) = resource.get()
        && let Some(name) = untrack(&url_getter) {
            let matched_id = items
                .iter()
                .find(|(_, n)| n.eq_ignore_ascii_case(&name))
                .map(|(id, _)| *id);
            setter.set(matched_id);
        }
    });
}

fn skeleton_library_grid() -> impl IntoView {
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
pub fn Library() -> impl IntoView {
    let (raw_search, set_raw_search) = signal(String::new());
    let debounced_search = crate::utils::use_debounced_signal(raw_search, 300);

    let (status_filter, set_status_filter)     = signal(Option::<i64>::None);
    let (tag_filter, set_tag_filter)           = signal(Option::<i64>::None);
    let (author_filter, set_author_filter)     = signal(Option::<i64>::None);
    let (artist_filter, set_artist_filter)     = signal(Option::<i64>::None);
    let (category_filter, set_category_filter) = signal(Option::<i64>::None);
    let (sort_order, set_sort_order)           = signal(MangaSortOrder::default());
    let (page, set_page)                       = signal(1i32);

    let all_tags       = Resource::new(|| (), |_| get_all_tags());
    let all_authors    = Resource::new(|| (), |_| get_all_authors());
    let all_artists    = Resource::new(|| (), |_| get_all_artists());
    let all_categories = Resource::new(|| (), |_| get_categories());

    let refresh_state        = expect_context::<RwSignal<RefreshState>>();
    let library_invalidation = expect_context::<RwSignal<u32>>();

    let library = Resource::new(
        move || (
            page.get(),
            { let s = debounced_search.get(); if s.is_empty() { None } else { Some(s) } },
            status_filter.get(),
            tag_filter.get(),
            author_filter.get(),
            artist_filter.get(),
            category_filter.get(),
            sort_order.get(),
            library_invalidation.get(),
        ),
        move |(p, search, status, tag, author, artist, category, sort, _)| async move {
            get_library(p, search, status, tag, author, artist, category, sort).await
        },
    );

    let (is_pending, set_pending) = signal(false);

    let query           = use_query_map();
    let author_from_url = move || query.with(|q| q.get("author").as_deref().map(str::to_string));
    let artist_from_url = move || query.with(|q| q.get("artist").as_deref().map(str::to_string));
    let tag_from_url    = move || query.with(|q| q.get("tag").as_deref().map(str::to_string));

    sync_filter_from_url(all_tags,    tag_from_url,    set_tag_filter);
    sync_filter_from_url(all_authors, author_from_url, set_author_filter);
    sync_filter_from_url(all_artists, artist_from_url, set_artist_filter);

    let is_running = move || matches!(refresh_state.get(), RefreshState::Running { .. });

    view! {
        <div class="library-page">
            <div class="library-header">
                <h1>"My Library"</h1>
                <div class="library-header__actions">
                    {move || match refresh_state.get() {
                        RefreshState::Running { completed, total } if total > 0 => {
                            let pct = (completed as f64 / total as f64 * 100.0) as u32;
                            view! {
                                <div class="refresh-progress">
                                    <div class="refresh-progress__track">
                                        <div
                                            class="refresh-progress__bar"
                                            style=format!("width: {}%", pct)
                                        ></div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        RefreshState::Running { .. } => view! {
                            <div class="refresh-progress">
                                <div class="refresh-progress__track">
                                    <div class="refresh-progress__bar refresh-progress__bar--indeterminate">
                                    </div>
                                </div>
                            </div>
                        }.into_any(),
                        RefreshState::Done { total, failed } => view! {
                            <span class="refresh-message">
                                {if failed == 0 {
                                    format!("Refreshed {} manga.", total)
                                } else {
                                    format!("Done — {} failed.", failed)
                                }}
                            </span>
                        }.into_any(),
                        RefreshState::Idle => ().into_any(),
                    }}
                    <button
                        class="refresh-button"
                        disabled=is_running
                        on:click=move |_| {
                            leptos::task::spawn_local(async move {
                                let _ = start_refresh_all().await;
                            });
                        }
                    >
                        {move || match refresh_state.get() {
                            RefreshState::Idle => "↻ Refresh All".to_string(),
                            RefreshState::Running { completed, total } if total > 0 =>
                                format!("Refreshing... {}/{}", completed, total),
                            RefreshState::Running { .. } => "Refreshing...".to_string(),
                            RefreshState::Done { .. } => "↻ Refresh All".to_string(),
                        }}
                    </button>
                </div>
            </div>

            <Suspense fallback=|| ()>
                {move || {
                    let cats = all_categories.get()
                        .and_then(|r| r.ok())
                        .unwrap_or_default();

                    if cats.is_empty() {
                        return ().into_any();
                    }

                    view! {
                        <div class="category-tabs">
                            <button
                                class=move || if category_filter.get().is_none() {
                                    "category-tab category-tab--active"
                                } else { "category-tab" }
                                on:click=move |_| {
                                    set_category_filter.set(None);
                                    set_page.set(1);
                                }
                            >
                                "All"
                            </button>
                            <For
                                each=move || cats.clone()
                                key=|c: &Category| c.id
                                children=move |cat| {
                                    let cat_id = cat.id;
                                    view! {
                                        <button
                                            class=move || if category_filter.get() == Some(cat_id) {
                                                "category-tab category-tab--active"
                                            } else { "category-tab" }
                                            on:click=move |_| {
                                                set_category_filter.set(Some(cat_id));
                                                set_page.set(1);
                                            }
                                        >
                                            {cat.name.clone()}
                                        </button>
                                    }
                                }
                            />
                        </div>
                    }.into_any()
                }}
            </Suspense>

            <div class="library-controls">
                <input
                    type="text"
                    placeholder="Search library..."
                    prop:value=move || raw_search.get()
                    on:input=move |ev| {
                        set_raw_search.set(event_target_value(&ev));
                        set_page.set(1);
                    }
                />

                <select on:change=move |ev| {
                    let val = event_target_value(&ev);
                    set_status_filter.set(val.parse::<i64>().ok());
                    set_page.set(1);
                }>
                    <option value="">"All Statuses"</option>
                    <option value="0">"Ongoing"</option>
                    <option value="1">"Completed"</option>
                    <option value="2">"Hiatus"</option>
                    <option value="3">"Cancelled"</option>
                    <option value="4">"Unknown"</option>
                </select>

                <Suspense fallback=|| ()>
                    {move || all_tags.get().map(|tags| {
                        let tags = tags.unwrap_or_default();
                        view! {
                            <select
                                prop:value=move || tag_filter.get()
                                    .map(|id| id.to_string())
                                    .unwrap_or_default()
                                on:change=move |ev| {
                                    let val = event_target_value(&ev);
                                    set_tag_filter.set(val.parse::<i64>().ok());
                                    set_page.set(1);
                                }
                            >
                                <option value="">"All Tags"</option>
                                <For
                                    each=move || tags.clone()
                                    key=|t| t.0
                                    children=move |tag| view! {
                                        <option value=tag.0.to_string()>{tag.1.clone()}</option>
                                    }
                                />
                            </select>
                        }
                    })}
                </Suspense>

                <Combobox
                    options=Signal::derive(move || {
                        all_authors.get().and_then(|r| r.ok()).unwrap_or_default()
                    })
                    value=Signal::derive(move || author_filter.get())
                    on_change=move |id| {
                        set_author_filter.set(id);
                        set_page.set(1);
                    }
                    placeholder="Search authors…"
                />

                <Combobox
                    options=Signal::derive(move || {
                        all_artists.get().and_then(|r| r.ok()).unwrap_or_default()
                    })
                    value=Signal::derive(move || artist_filter.get())
                    on_change=move |id| {
                        set_artist_filter.set(id);
                        set_page.set(1);
                    }
                    placeholder="Search artists…"
                />

                <select on:change=move |ev| {
                    set_sort_order.set(
                        MangaSortOrder::from_select_value(&event_target_value(&ev))
                    );
                    set_page.set(1);
                }>
                    <option value="updated_desc">"Recently Updated ↓"</option>
                    <option value="updated_asc">"Recently Updated ↑"</option>
                    <option value="added_desc">"Recently Added ↓"</option>
                    <option value="added_asc">"Recently Added ↑"</option>
                    <option value="name_asc">"Name A-Z"</option>
                    <option value="name_desc">"Name Z-A"</option>
                </select>
            </div>

            <div class="page-loading-bar" class:page-loading-bar--active=move || is_pending.get()>
                <div class="page-loading-bar__fill"></div>
            </div>

            // RESTORED: `set_pending` captures the loading state from inside the transition boundaries
            <Transition fallback=skeleton_library_grid set_pending>
                {move || library.get().map(|res| match res {
                    Err(e) => view! {
                        <p class="error">"Error: " {e.to_string()}</p>
                    }.into_any(),
                    Ok(page_data) if page_data.items.is_empty() => view! {
                        <p class="empty">"No manga found. Try adjusting your filters."</p>
                    }.into_any(),
                    Ok(page_data) => {
                        let items    = page_data.items.clone();
                        let has_next = page_data.has_next_page;
                        view! {
                            <div
                                class="manga-grid"
                                class:content--stale=move || is_pending.get()
                            >
                                <For
                                    each=move || items.clone()
                                    key=|m| m.id.clone()
                                    children=move |manga| view! {
                                        <div class="manga-card">
                                            <A href=format!("/manga/{}", manga.id)>
                                                <CoverImage
                                                    url=manga.cover_url
                                                    alt=manga.title.clone()
                                                />
                                                <div class="title">{manga.title}</div>
                                            </A>
                                        </div>
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