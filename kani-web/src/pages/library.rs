use crate::{server_fns::{get_all_artists, get_all_authors, get_all_categories, get_all_tags, get_library, proxy_url, start_refresh_all}, types::MangaSortOrder};
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};

#[component]
pub fn Library() -> impl IntoView {
    let (raw_search, set_raw_search) = signal(String::new());
    let (debounced_search, set_debounced_search) = signal(Option::<String>::None);

    Effect::new(move |_| {
        let val = raw_search.get();
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(300).await;
            if raw_search.get_untracked() == val {
                let search = if val.is_empty() { None } else { Some(val) };
                set_debounced_search.set(search);
            }
        });
    });

    let (status_filter, set_status_filter) = signal(Option::<i64>::None);
    let (tag_filter, set_tag_filter) = signal(Option::<i64>::None);
    let (author_filter, set_author_filter) = signal(Option::<i64>::None);
    let (artist_filter, set_artist_filter) = signal(Option::<i64>::None);
    let (category_filter, set_category_filter) = signal(Option::<i64>::None);
    
    let (sort_order, set_sort_order) = signal(MangaSortOrder::default());
    
    let (page, set_page) = signal(1i32);

    let all_tags = Resource::new(|| (), |_| get_all_tags());
    let all_authors = Resource::new(|| (), |_| get_all_authors());
    let all_artists = Resource::new(|| (), |_| get_all_artists());
    let all_categories = Resource::new(|| (), |_| get_all_categories());

    let library = Resource::new(
        move || (
            page.get(),
            debounced_search.get(),
            status_filter.get(),
            tag_filter.get(),
            author_filter.get(),
            artist_filter.get(),
            category_filter.get(),
            sort_order.get(),
        ),
        move |(p, search, status, tag, author, artist, category, sort)| async move {
            get_library(p, search, status, tag, author, artist, category, sort).await
        },
    );

    let query = use_query_map();

    let author_from_url = move || query.with(|q| q.get("author").as_deref().map(str::to_string));
    let artist_from_url = move || query.with(|q| q.get("artist").as_deref().map(str::to_string));
    let tag_from_url    = move || query.with(|q| q.get("tag").as_deref().map(str::to_string));

    macro_rules! sync_filter {
        ($resource:expr, $url_getter:expr, $signal_setter:expr) => {
            Effect::new(move |_| {
                if let Some(Ok(items)) = $resource.get()
                    && let Some(name) = $url_getter() {
                        let matched_id = items.iter()
                            .find(|(_, n)| n.eq_ignore_ascii_case(&name))
                            .map(|(id, _)| *id);
                        $signal_setter.set(matched_id);
                }
            });
        };
    }

    sync_filter!(all_tags, tag_from_url, set_tag_filter);
    sync_filter!(all_authors, author_from_url, set_author_filter);
    sync_filter!(all_artists, artist_from_url, set_artist_filter);

    #[derive(Clone, PartialEq)]
    enum RefreshState {
        Idle,
        Running { completed: usize, total: usize },
        Done { total: usize, failed: usize },
    }

    let (refresh_state, set_refresh_state) = signal(RefreshState::Idle);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::prelude::*;
            use web_sys::{EventSource, MessageEvent};

            let es = match EventSource::new("/rest/refresh/progress") {
                Ok(es) => es,
                Err(_) => return,
            };

            let es_for_close = es.clone();
            let on_close = Closure::<dyn FnMut(MessageEvent)>::new(
                move |_: MessageEvent| { es_for_close.close(); }
            );
            es.add_event_listener_with_callback(
                "close", on_close.as_ref().unchecked_ref()
            ).ok();
            on_close.forget();

            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(
                move |msg: MessageEvent| {
                    use crate::events::RefreshProgressEvent;

                    let data = match msg.data().as_string() {
                        Some(d) => d,
                        None => return,
                    };

                    if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&data)
                        && raw["type"] == "state_snapshot" {
                            if raw["is_refreshing"].as_bool().unwrap_or(false) {
                                set_refresh_state.set(RefreshState::Running {
                                    completed: 0,
                                    total: 0,
                                });
                            }
                            return;
                        }

                    let event: RefreshProgressEvent = match serde_json::from_str(&data) {
                        Ok(e) => e,
                        Err(_) => return,
                    };

                    match event {
                        RefreshProgressEvent::Started { total } => {
                            set_refresh_state.set(RefreshState::Running {
                                completed: 0,
                                total,
                            });
                        }
                        RefreshProgressEvent::MangaRefreshed { completed, total, .. } => {
                            set_refresh_state.set(RefreshState::Running { completed, total });
                        }
                        RefreshProgressEvent::Completed { total, failed } => {
                            set_refresh_state.set(RefreshState::Done { total, failed });
                            library.refetch();
                            set_timeout(
                                move || set_refresh_state.set(RefreshState::Idle),
                                std::time::Duration::from_secs(5),
                            );
                        }
                    }
                }
            );

            es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();

            let es_clone = es.clone();
            on_cleanup(move || es_clone.close());
            drop(es);
        }
    });

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
                                    <div class="refresh-progress__bar refresh-progress__bar--indeterminate"></div>
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
                            RefreshState::Running { .. } =>
                                "Refreshing...".to_string(),
                            RefreshState::Done { .. } => "↻ Refresh All".to_string(),
                        }}
                    </button>
                </div>
            </div>
            <Suspense fallback=move || view! { <p>"Loading library..."</p> }>
                {move || library.get().map(|res| match res {
                    Ok(library) => {
                        if library.items.is_empty() {
                            view! { <p>"Your library is empty. Go add some manga!"</p> }.into_any()
                        } else {                            
                            view! {
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
                                                    prop:value=move || tag_filter.get().map(|id| id.to_string()).unwrap_or_default()
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

                                    // Todo: Implement autocomplete
                                    <input
                                        type="text"
                                        list="author-options"
                                        placeholder="Search authors..."
                                        on:change=move |ev| {
                                            let name = event_target_value(&ev);
                                            let matched_id = all_authors.get()
                                                .and_then(|r| r.ok())
                                                .unwrap_or_default()
                                                .into_iter()
                                                .find(|(_, n)| n.eq_ignore_ascii_case(&name))
                                                .map(|(id, _)| id);
                                            set_author_filter.set(matched_id);
                                            set_page.set(1);
                                        }
                                    />
                                    <datalist id="author-options">
                                        <For
                                            each=move || all_authors.get().and_then(|r| r.ok()).unwrap_or_default()
                                            key=|(id, _)| *id
                                            children=|(_, name)| view! {
                                                <option value=name.clone()/>
                                            }
                                        />
                                    </datalist>

                                    <input
                                        type="text"
                                        list="artist-options"
                                        placeholder="Search artists..."
                                        on:change=move |ev| {
                                            let name = event_target_value(&ev);
                                            let matched_id = all_artists.get()
                                                .and_then(|r| r.ok())
                                                .unwrap_or_default()
                                                .into_iter()
                                                .find(|(_, n)| n.eq_ignore_ascii_case(&name))
                                                .map(|(id, _)| id);
                                            set_artist_filter.set(matched_id);
                                            set_page.set(1);
                                        }
                                    />
                                    <datalist id="artist-options">
                                        <For
                                            each=move || all_artists.get().and_then(|r| r.ok()).unwrap_or_default()
                                            key=|(id, _)| *id
                                            children=|(_, name)| view! {
                                                <option value=name.clone()/>
                                            }
                                        />
                                    </datalist>

                                    <Suspense fallback=|| ()>
                                        {move || all_categories.get().map(|tags| {
                                            let tags = tags.unwrap_or_default();
                                            view! {
                                                <select
                                                    prop:value=move || category_filter.get().map(|id| id.to_string()).unwrap_or_default()
                                                    on:change=move |ev| {
                                                        let val = event_target_value(&ev);
                                                        set_category_filter.set(val.parse::<i64>().ok());
                                                        set_page.set(1);
                                                    }
                                                >
                                                    <option value="">"All Categories"</option>
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

                                    <select on:change=move |ev| {
                                        set_sort_order.set(MangaSortOrder::from_select_value(&event_target_value(&ev)));
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

                                <div class="manga-grid">
                                    <For
                                        each=move || library.items.clone()
                                        key=|(m, _)| m.id.clone()
                                        children=move |(manga, base_url)| view! {
                                            <div class="manga-card">
                                                <A href=format!("/manga/{}", manga.id)>
                                                    <div class="cover">
                                                        {match manga.cover_url {
                                                            Some(url) => {
                                                                let src = proxy_url(&url, &base_url);
                                                                view! { <img src=src alt=manga.title.clone() /> }.into_any()
                                                            },
                                                            None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                                                        }}
                                                    </div>
                                                    <div class="title">{manga.title}</div>
                                                </A>
                                            </div>
                                        }
                                    />
                                </div>
                                
                                <div class="pagination">
                                    <button on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1)) disabled=move || page.get() <= 1>"Prev"</button>
                                    <span>" Page " {page} </span>
                                    <button on:click=move |_| set_page.update(|p| *p += 1) disabled=move || !library.has_next_page>"Next"</button>
                                </div>
                            }.into_any()
                        }
                    },
                    Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any()
                })}
            </Suspense>
        </div>
    }
}