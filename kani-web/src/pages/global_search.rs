use leptos::prelude::*;
use leptos::web_sys;

use crate::{server_fns::{fetch_sources, global_search, toggle_source_favourite}, types::SearchScope};
use leptos_router::{components::A};

#[component]
pub fn GlobalSearch() -> impl IntoView {
    let (raw_input, set_raw_input) = signal(String::new());
    let (committed_query, set_committed_query) = signal(String::new());
    let (page, set_page) = signal(1i32);

    let (scope, set_scope) = signal(SearchScope::AllEnabled);

    Effect::new(move |_| {
        let val = raw_input.get();
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(300).await;
            if raw_input.get_untracked() == val {
                set_committed_query.set(val);
                set_page.set(1);
            }
        });
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

          // ── Search bar ──────────────────────────────────────────────────
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

          // ── Scope + source filter chips ─────────────────────────────────
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

                                  let (is_fav, set_is_fav) = signal(source.favourited);

                                  let toggle_fav = move |ev: web_sys::MouseEvent| {
                                      ev.stop_propagation();
                                      let new_val = !is_fav.get();
                                      set_is_fav.set(new_val);
                                      leptos::task::spawn_local(async move {
                                          if toggle_source_favourite(source_id, new_val).await.is_err() {
                                              set_is_fav.set(!new_val);
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
                                          <span
                                              class=move || if is_fav.get() { "star star--on" } else { "star" }
                                              on:click=toggle_fav
                                          >
                                              {move || if is_fav.get() { "★" } else { "☆" }}
                                          </span>
                                      </button>
                                  }
                              }
                          />
                      }
                  })}
              </Suspense>

          </div>

          // ── Results ─────────────────────────────────────────────────────
          <Suspense fallback=move || view! { <div class="spinner">"Searching..."</div> }>
              {move || search_results.get().map(|res| match res {
                  Err(e) => view! {
                      <p class="error">"Search error: " {e.to_string()}</p>
                  }.into_any(),

                  Ok(ref results) if results.is_empty() && !committed_query.get().is_empty() => {
                      view! { <p class="empty">"No results found."</p> }.into_any()
                  }

                  Ok(results) => view! {
                      <div class="search-results">
                          <For
                              each=move || results.clone()
                              key=|r| r.source_id
                              children=move |result| {
                                  let source_id = result.source_id;
                                  view! {
                                      <section class="source-section">
                                          <h3 class="source-section__header">
                                              {result.source_name.clone()}
                                          </h3>

                                          {if result.manga.is_empty() {
                                              view! {
                                                  <p class="source-section__empty">
                                                      "No results from this source."
                                                  </p>
                                              }.into_any()
                                          } else {
                                              view! {
                                                  <div class="manga-grid">
                                                      <For
                                                          each=move || result.manga.clone()
                                                          key=|m| m.id.clone()
                                                          children=move |manga| {
                                                              let href = format!(
                                                                  "/source/{}/manga/{}",
                                                                  source_id, manga.id
                                                              );
                                                              let cover = manga.cover_url.clone();
                                                              view! {
                                                                  <A href=href>
                                                                      <div class="manga-card">
                                                                          {match cover {
                                                                              Some(url) => view! {
                                                                                  <img src=url alt=manga.title.clone() />
                                                                              }.into_any(),
                                                                              None => view! {
                                                                                  <div class="no-cover">"No Cover"</div>
                                                                              }.into_any(),
                                                                          }}
                                                                          <p>{manga.title.clone()}</p>
                                                                      </div>
                                                                  </A>
                                                              }
                                                          }
                                                      />
                                                  </div>
                                              }.into_any()
                                          }}
                                      </section>
                                  }
                              }
                          />

                          // ── Pagination ──────────────────────────────────
                          <div class="pagination">
                              {move || (page.get() > 1).then(|| view! {
                                  <button on:click=move |_| set_page.update(|p| *p -= 1)>
                                      "← Previous"
                                  </button>
                              })}
                              <span>"Page " {page}</span>
                              <button on:click=move |_| set_page.update(|p| *p += 1)>
                                  "Load more →"
                              </button>
                          </div>
                      </div>
                  }.into_any()
              })}
          </Suspense>

      </div>
  }
}