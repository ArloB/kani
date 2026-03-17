use leptos::prelude::*;

use crate::{pages::components::{collapsible_panel::CollapsiblePanel, cover_image::CoverImage, pagination::Pagination}, server_fns::{fetch_sources, global_search}, types::SearchScope};
use leptos_router::{components::A};

#[component]
pub fn GlobalSearch() -> impl IntoView {
    let (raw_input, set_raw_input) = signal(String::new());
    let (page, set_page) = signal(1i32);

    let (scope, set_scope) = signal(SearchScope::AllEnabled);

    let committed_query = crate::utils::use_debounced_signal(raw_input, 300);

    Effect::new(move |prev: Option<String>| {
        let val = committed_query.get();
        if prev.is_some() {
            set_page.set(1);
        }
        val
    });

    let sources = Resource::new(|| (), |_| fetch_sources());

    let search_results = Resource::new(
        move || (committed_query.get(), scope.get(), page.get()),
        move |(q, scope, p)| async move {
            if q.is_empty() {
                return Ok(vec![]);
            }
            global_search(q, scope, p).await
        },
    );

    view! {
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
                  } else {
                      "chip"
                  }
                  on:click=move |_| set_scope.set(SearchScope::FavouritedOnly)
              >
                  "★ Favourites"
              </button>

              <button
                  class=move || if matches!(scope.get(), SearchScope::AllEnabled) {
                      "chip chip--active"
                  } else {
                      "chip"
                  }
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
          </div>

          <Suspense fallback=move || view! { <div class="spinner">"Searching..."</div> }>
              {move || search_results.get().map(|res| match res {
                  Err(e) => view! {
                      <p class="error">"Search error: " {e.to_string()}</p>
                  }.into_any(),

                  Ok(ref results) if results.is_empty() && !committed_query.get().is_empty() => {
                      view! { <p class="empty">"No results found."</p> }.into_any()
                  }

                  Ok(results) => {
                    let has_next = {
                        let results = results.clone();
                        Signal::derive(move || results.iter().any(|r| r.has_next_page))
                    };

                    view! {
                        <div class="search-results">
                            <For
                                each=move || results.clone()
                                key=|r| r.source_id
                                children=move |result| {
                                    let source_id = result.source_id;
                                    let count = result.manga.len();
                                    let label = if result.manga.is_empty() {
                                        result.source_name.clone()
                                    } else {
                                        format!(
                                            "{} ({}{}", result.source_name, count,
                                            if result.has_next_page { "+)" } else { ")" }
                                        )
                                    };
                                    let manga = result.manga.clone();
                                    let is_empty = manga.is_empty();

                                    view! {
                                        <CollapsiblePanel label=label open=true>
                                            {if is_empty {
                                                view! {
                                                    <p class="source-section__empty">
                                                        "No results."
                                                    </p>
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

                            <Pagination page set_page has_next=has_next/>
                        </div>
                    }.into_any()
                  }
              })}
          </Suspense>
      </div>
    }
}