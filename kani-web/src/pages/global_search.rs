use leptos::prelude::*;
use leptos_meta::Title;

use crate::{
    pages::components::{
        collapsible_panel::CollapsiblePanel,
        cover_image::CoverImage,
        pagination::Pagination,
        permission_handlers::RequirePermission,
    },
    server_fns::{fetch_sources, global_search},
    types::SearchScope,
};
use leptos_router::components::A;

fn skeleton_search_results() -> impl IntoView {
    view! {
        <div class="search-results">
            {(0..3).map(|_| view! {
                <div style="display: flex; flex-direction: column; gap: var(--sp-3)">
                    <div class="skeleton-row skeleton-row--xs" style="width: 160px"></div>
                    <div class="manga-scroll-row">
                        {(0..5).map(|_| view! {
                            <div class="skeleton-card" style="flex: 0 0 var(--grid-min-w); width: var(--grid-min-w)">
                                <div class="skeleton-card__cover"></div>
                                <div class="skeleton-card__title skeleton-card__title--long"></div>
                            </div>
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            }).collect::<Vec<_>>()}
        </div>
    }
}

#[component]
pub fn GlobalSearch() -> impl IntoView {
    let (raw_input, set_raw_input) = signal(String::new());
    let (page, set_page)           = signal(1i32);
    let (scope, set_scope)         = signal(SearchScope::AllEnabled);

    let initial_page_size = crate::utils::get_local_string("kani_search_page_size")
        .parse::<i32>()
        .unwrap_or(24);
    let (page_size, set_page_size) = signal(initial_page_size);

    let committed_query = crate::utils::use_debounced_signal(raw_input, 600);

    Effect::new(move |prev: Option<String>| {
        let val = committed_query.get();
        if prev.is_some() {
            set_page.set(1);
        }
        val
    });

    let sources = Resource::new(|| (), |_| fetch_sources());

    let search_results = Resource::new(
        move || (committed_query.get(), scope.get(), page.get(), page_size.get()),
        move |(q, scope, p, ps)| async move {
            if q.is_empty() {
                return Ok(vec![]);
            }
            global_search(q, scope, p, ps).await
        },
    );

    let (is_pending, set_pending) = signal(false);

    view! {
      <Title text=move || {
          let q = committed_query.get();
          if q.is_empty() { "Search - Kani".into() } else { format!("{q} - Search - Kani") }
      }/>
      <RequirePermission permission="source:browse">
        <div class="global-search-page">
            <div class="search-bar">
                <span class="search-icon">"🔍"</span>
                <input
                    type="text"
                    placeholder="Search all sources..."
                    prop:value=raw_input
                    on:input=move |ev| set_raw_input.set(event_target_value(&ev))
                    autofocus=true
                />
            </div>

            <div class="source-filters">
                <button
                    class=move || if matches!(scope.get(), SearchScope::FavouritedOnly) {
                        "chip chip--active"
                    } else { "chip" }
                    on:click=move |_| set_scope.set(SearchScope::FavouritedOnly)
                >
                    "★ Favourites"
                </button>

                <button
                    class=move || if matches!(scope.get(), SearchScope::AllEnabled) {
                        "chip chip--active"
                    } else { "chip" }
                    on:click=move |_| set_scope.set(SearchScope::AllEnabled)
                >
                    "All"
                </button>

                <Suspense fallback=|| ()>
                    {move || sources.get().map(|res| {
                        let sources = res.unwrap_or_default();
                        view! {
                            <For
                                each=move || sources.clone()
                                key=|s| s.id
                                children=move |source| {
                                    let source_id = source.id;

                                    let toggle_source = move |_| {
                                        set_scope.update(|current| {
                                            let mut ids = match current {
                                                SearchScope::Sources(ids) => ids.clone(),
                                                _ => vec![],
                                            };
                                            if ids.contains(&source_id) {
                                                ids.retain(|&id| id != source_id);
                                                *current = if ids.is_empty() {
                                                    SearchScope::AllEnabled
                                                } else {
                                                    SearchScope::Sources(ids)
                                                };
                                            } else {
                                                ids.push(source_id);
                                                *current = SearchScope::Sources(ids);
                                            }
                                        });
                                    };

                                    view! {
                                        <button
                                            class=move || {
                                                let active = matches!(
                                                    scope.get(),
                                                    SearchScope::Sources(ref ids) if ids.contains(&source_id)
                                                );
                                                if active { "chip chip--active" } else { "chip" }
                                            }
                                            on:click=toggle_source
                                        >
                                            {source.name.clone()}
                                        </button>
                                    }
                                }
                            />
                        }
                    })}
                </Suspense>

                <select
                    class="source-filters__page-size"
                    prop:value=move || page_size.get().to_string()
                    on:change=move |ev| {
                        let ps = event_target_value(&ev).parse::<i32>().unwrap_or(24);
                        crate::utils::set_local_string("kani_search_page_size", &ps.to_string());
                        set_page_size.set(ps);
                        set_page.set(1);
                    }
                >
                    <option value="12">"12 per page"</option>
                    <option value="24">"24 per page"</option>
                    <option value="48">"48 per page"</option>
                </select>
            </div>

            <div class="page-loading-bar" class:page-loading-bar--active=move || is_pending.get()>
                <div class="page-loading-bar__fill"></div>
            </div>

            <Transition fallback=skeleton_search_results set_pending>
                {move || search_results.get().map(|res| match res {
                    Err(e) => view! {
                        <p class="error">"Search error: " {e.to_string()}</p>
                    }.into_any(),

                    Ok(ref results) if results.is_empty() && !committed_query.get().is_empty() => {
                        view! { <p class="empty">"No results found."</p> }.into_any()
                    }

                    Ok(ref results) if results.is_empty() => {
                        ().into_any()
                    }

                    Ok(results) => {
                        let has_next = {
                            let results = results.clone();
                            Signal::derive(move || results.iter().any(|r| r.has_next_page))
                        };

                        view! {
                            <div
                                class="search-results"
                                class:content--stale=move || is_pending.get()
                            >
                                <For
                                    each=move || results.clone()
                                    key=|r| r.source_id
                                    children=move |result| {
                                        let source_id = result.source_id;
                                        let count     = result.manga.len();
                                        let label = if result.manga.is_empty() {
                                            result.source_name.clone()
                                        } else {
                                            format!(
                                                "{} ({}{}", result.source_name, count,
                                                if result.has_next_page { "+)" } else { ")" }
                                            )
                                        };
                                        let manga    = result.manga.clone();
                                        let is_empty = manga.is_empty();

                                        view! {
                                            <CollapsiblePanel label=label open=true>
                                                {if is_empty {
                                                    view! {
                                                        <p class="source-section__empty">"No results."</p>
                                                    }.into_any()
                                                } else {
                                                    let manga = manga.clone();
                                                    view! {
                                                        <div class="manga-scroll-row">
                                                            <For
                                                                each=move || manga.clone()
                                                                key=|m| m.id.clone()
                                                                children=move |manga| {
                                                                    let href = format!(
                                                                        "/source/{}/manga/{}",
                                                                        source_id, manga.id
                                                                    );
                                                                    view! {
                                                                        <A href=href attr:class="manga-card">
                                                                            <CoverImage
                                                                                url=manga.cover_url.clone()
                                                                                alt=manga.title.clone()
                                                                            />
                                                                            <p>{manga.title.clone()}</p>
                                                                        </A>
                                                                    }
                                                                }
                                                            />
                                                        </div>
                                                    }.into_any()
                                                }}
                                            </CollapsiblePanel>
                                        }
                                    }
                                />

                                <Pagination page set_page has_next=has_next />
                            </div>
                        }.into_any()
                    }
                })}
            </Transition>
        </div>
      </RequirePermission>
    }
}