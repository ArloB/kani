use crate::pages::components::collapsible_panel::CollapsiblePanel;
use crate::server_fns::{migrate_manga as migrate_manga_fn, preview_migration};
use crate::types::{GlobalSearchResult, MangaListItem, MigrationStep, SearchScope};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[allow(non_snake_case)]
#[component]
pub fn MigrationDialogue(
    db_id: i64,
    current_source_id: i64,
    current_source_name: String,
    current_title: String,
    migration_step: ReadSignal<MigrationStep>,
    set_migration_step: WriteSignal<MigrationStep>,
    mig_scope: ReadSignal<SearchScope>,
    set_mig_scope: WriteSignal<SearchScope>,
    mig_query: ReadSignal<String>,
    set_mig_query: WriteSignal<String>,
    mig_error: ReadSignal<Option<String>>,
    set_mig_error: WriteSignal<Option<String>>,
    search_resource: Resource<Result<Vec<GlobalSearchResult>, ServerFnError>>,
    on_complete: Callback<()>,
) -> impl IntoView {
    let title_v  = StoredValue::new(current_title);
    let s_name_v = StoredValue::new(current_source_name);

    let set_step = set_migration_step;
    let set_err  = set_mig_error;

    view! {
        <Show when=move || migration_step.get() != MigrationStep::Closed fallback=|| ()>
            <div
                class="modal-overlay"
                on:click=move |e| {
                    if e.target() == e.current_target() {
                        set_step.set(MigrationStep::Closed);
                    }
                }
            >
                <div class="modal modal--wide">
                    <div class="modal-header">
                        <h2>
                            {move || match migration_step.get() {
                                MigrationStep::Search      => "Migrate Source — Search".to_string(),
                                MigrationStep::Previewing  => "Confirm Migration".to_string(),
                                MigrationStep::Preview(..) => "Confirm Migration".to_string(),
                                MigrationStep::Confirming  => "Migrating…".to_string(),
                                MigrationStep::Done(_)     => "Migration Complete".to_string(),
                                MigrationStep::Closed      => String::new(),
                            }}
                        </h2>
                        <button
                            class="modal-close"
                            on:click=move |_| set_step.set(MigrationStep::Closed)
                        >
                            "×"
                        </button>
                    </div>
                    <div class="modal-body">
                        {move || match migration_step.get() {
                            MigrationStep::Search => view! {

                                <input
                                    type="search"
                                    prop:value=move || mig_query.get()
                                    placeholder="Manga title…"
                                    on:input=move |e| set_mig_query.set(event_target_value(&e))
                                />

                                <div class="source-filters">
                                    <button
                                        class=move || if matches!(
                                            mig_scope.get(), SearchScope::FavouritedOnly
                                        ) { "chip chip--active" } else { "chip" }
                                        on:click=move |_| set_mig_scope.set(SearchScope::FavouritedOnly)
                                    >
                                        "★ Favourites"
                                    </button>
                                    <button
                                        class=move || if matches!(
                                            mig_scope.get(), SearchScope::AllEnabled
                                        ) { "chip chip--active" } else { "chip" }
                                        on:click=move |_| set_mig_scope.set(SearchScope::AllEnabled)
                                    >
                                        "All"
                                    </button>
                                </div>

                                <Suspense fallback=|| view! { <p class="spinner">"Searching…"</p> }>
                                    {move || search_resource.get().map(|res| match res {
                                        Err(e) => view! {
                                            <p class="error">"Search error: " {e.to_string()}</p>
                                        }.into_any(),

                                        Ok(results)
                                            if results.is_empty()
                                            && !mig_query.get_untracked().is_empty() =>
                                        {
                                            view! {
                                                <p class="empty">"No results found."</p>
                                            }.into_any()
                                        }

                                        Ok(results) => {
                                            let (mut current, mut others): (Vec<_>, Vec<_>) =
                                                results.into_iter()
                                                    .partition(|r| r.source_id == current_source_id);
                                            others.sort_by(|a, b| a.source_name.cmp(&b.source_name));
                                            let ordered = current
                                                .drain(..)
                                                .chain(others)
                                                .collect::<Vec<_>>();
                                            let stored_ordered = StoredValue::new(ordered);

                                            view! {
                                                <div class="search-results">
                                                    <For
                                                        each=move || stored_ordered.get_value()
                                                        key=|r| r.source_id
                                                        children=move |res| {
                                                            let is_current =
                                                                res.source_id == current_source_id;
                                                            view! {
                                                                <SourceSection
                                                                    result=res
                                                                    is_current=is_current
                                                                    db_id=db_id
                                                                    set_step=set_step
                                                                    set_err=set_err
                                                                />
                                                            }
                                                        }
                                                    />
                                                </div>
                                            }.into_any()
                                        }
                                    })}
                                </Suspense>

                                {move || mig_error.get().map(|e| view! {
                                    <div class="modal-notice modal-notice--error">{e}</div>
                                })}

                            }.into_any(),

                            MigrationStep::Previewing => {
                                let c_title  = title_v.get_value();
                                let c_source = s_name_v.get_value();
                                view! {
                                    <div class="migration-comparison">
                                        <div class="migration-comparison__source">
                                            <strong>{c_title}</strong>
                                            <span>{c_source}</span>
                                        </div>
                                        <span class="migration-comparison__arrow">"→"</span>
                                        <div class="migration-comparison__source">
                                            <div class="migration-skeleton__line migration-skeleton__line--title"></div>
                                            <div class="migration-skeleton__line migration-skeleton__line--sub"></div>
                                        </div>
                                    </div>
                                    <div class="migration-stats">
                                        <div class="migration-skeleton__stat"></div>
                                        <div class="migration-skeleton__stat"></div>
                                        <div class="migration-skeleton__stat"></div>
                                    </div>
                                }.into_any()
                            },

                            MigrationStep::Preview(preview, _sid, _mid) => {
                                let c_title  = title_v.get_value();
                                let c_source = s_name_v.get_value();
                                let t_title  = preview.target_title.clone();
                                let at_risk  = preview.downloaded_chapters_at_risk;

                                view! {
                                    <div class="migration-comparison">
                                        <div class="migration-comparison__source">
                                            <strong>{c_title}</strong>
                                            <span>{c_source}</span>
                                        </div>
                                        <span class="migration-comparison__arrow">"→"</span>
                                        <div class="migration-comparison__source">
                                            <strong>{t_title}</strong>
                                            <span>"Target source"</span>
                                        </div>
                                    </div>

                                    <div class="migration-stats">
                                        <div class="migration-stat-row migration-stat-row--positive">
                                            <span class="label">"Chapters matched"</span>
                                            <span class="value">{preview.chapters_matched}</span>
                                        </div>
                                        <div class="migration-stat-row">
                                            <span class="label">"New chapters"</span>
                                            <span class="value">{preview.chapters_new}</span>
                                        </div>
                                        <div class="migration-stat-row">
                                            <span class="label">"Chapters to remove"</span>
                                            <span class="value">{preview.chapters_orphaned}</span>
                                        </div>
                                        {if at_risk > 0 {
                                            view! {
                                                <div class="migration-stat-row migration-stat-row--warn">
                                                    <span class="label">"⚠ Downloaded at risk"</span>
                                                    <span class="value">{at_risk}</span>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! { <span/> }.into_any()
                                        }}
                                    </div>

                                    {if at_risk > 0 {
                                        view! {
                                            <div class="modal-notice modal-notice--warn">
                                                "Some orphaned chapters have already been downloaded. "
                                                "Their CBZ files will be permanently deleted when you confirm."
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! { <span/> }.into_any()
                                    }}

                                    {move || mig_error.get().map(|e| view! {
                                        <div class="modal-notice modal-notice--error">{e}</div>
                                    })}

                                }.into_any()
                            },

                            MigrationStep::Confirming => view! {
                                <p class="spinner">"Migrating, please wait…"</p>
                            }.into_any(),

                            MigrationStep::Done(result) => view! {
                                <div class="migration-result">
                                    <div class="modal-notice modal-notice--success">
                                        "Migration complete!"
                                    </div>
                                    <div class="migration-stats">
                                        <div class="migration-stat-row">
                                            <span class="label">"Chapters matched"</span>
                                            <span class="value">{result.chapters_matched}</span>
                                        </div>
                                        <div class="migration-stat-row">
                                            <span class="label">"New chapters added"</span>
                                            <span class="value">{result.chapters_new}</span>
                                        </div>
                                        <div class="migration-stat-row">
                                            <span class="label">"Chapters removed"</span>
                                            <span class="value">{result.chapters_orphaned}</span>
                                        </div>
                                    </div>
                                </div>
                            }.into_any(),

                            MigrationStep::Closed => view! { <span/> }.into_any(),
                        }}
                    </div>

                    <div class="modal-footer">
                        {move || match migration_step.get() {

                            MigrationStep::Search => view! {
                                <button
                                    class="migration-cancel-btn"
                                    on:click=move |_| set_step.set(MigrationStep::Closed)
                                >
                                    "Cancel"
                                </button>
                            }.into_any(),

                            MigrationStep::Previewing => view! {
                                <button
                                    class="migration-cancel-btn"
                                    on:click=move |_| set_step.set(MigrationStep::Search)
                                >
                                    "← Back"
                                </button>
                            }.into_any(),

                            MigrationStep::Preview(_, target_sid, target_mid) => {
                                let mid = target_mid.clone();
                                view! {
                                    <button
                                        class="migration-cancel-btn"
                                        on:click=move |_| {
                                            set_err.set(None);
                                            set_step.set(MigrationStep::Search);
                                        }
                                    >
                                        "← Back"
                                    </button>
                                    <button
                                        class="migration-confirm-btn"
                                        on:click=move |_| {
                                            let mid     = mid.clone();
                                            let on_done = on_complete;
                                            set_step.set(MigrationStep::Confirming);
                                            spawn_local(async move {
                                                match migrate_manga_fn(db_id, target_sid, mid).await {
                                                    Ok(res) => {
                                                        on_done.run(());
                                                        set_step.set(MigrationStep::Done(res));
                                                    }
                                                    Err(e) => {
                                                        set_err.set(Some(e.to_string()));
                                                        set_step.set(MigrationStep::Search);
                                                    }
                                                }
                                            });
                                        }
                                    >
                                        "Confirm Migration"
                                    </button>
                                }.into_any()
                            },

                            MigrationStep::Confirming => view! { <span/> }.into_any(),

                            MigrationStep::Done(_) => view! {
                                <button
                                    class="migration-cancel-btn"
                                    on:click=move |_| set_step.set(MigrationStep::Closed)
                                >
                                    "Close"
                                </button>
                            }.into_any(),

                            _ => view! { <span/> }.into_any(),
                        }}
                    </div>

                </div>
            </div>
        </Show>
    }
}

#[component]
fn SourceSection(
    result: GlobalSearchResult,
    is_current: bool,
    db_id: i64,
    set_step: WriteSignal<MigrationStep>,
    set_err: WriteSignal<Option<String>>,
) -> impl IntoView {
    let sid      = result.source_id;
    let count    = result.manga.len();
    let has_next = result.has_next_page;

    let label = if is_current {
        format!("{} (current)", result.source_name)
    } else if count > 0 {
        format!(
            "{} ({}{}",
            result.source_name,
            count,
            if has_next { "+)" } else { ")" }
        )
    } else {
        result.source_name.clone()
    };

    let manga_data = StoredValue::new(result.manga);
    let is_empty   = count == 0;

    view! {
        <CollapsiblePanel label=label open=true>
            {if is_empty {
                view! {
                    <p class="source-section__empty">"No results."</p>
                }.into_any()
            } else {
                view! {
                    <div class="manga-scroll-row">
                        <For
                            each=move || manga_data.get_value()
                            key=|m| m.id.clone()
                            children=move |m| view! {
                                <MangaCard
                                    manga=m
                                    is_current=is_current
                                    db_id=db_id
                                    sid=sid
                                    set_step=set_step
                                    set_err=set_err
                                />
                            }
                        />
                    </div>
                }.into_any()
            }}
        </CollapsiblePanel>
    }
}

#[component]
fn MangaCard(
    manga: MangaListItem,
    is_current: bool,
    db_id: i64,
    sid: i64,
    set_step: WriteSignal<MigrationStep>,
    set_err: WriteSignal<Option<String>>,
) -> impl IntoView {
    let id    = manga.id.clone();
    let title = manga.title.clone();
    let cover = manga.cover_url.clone();

    view! {
        <div
            class=if is_current { "manga-card manga-card--current" } else { "manga-card" }
            on:click=move |_| {
                if is_current { return; }
                let id = id.clone();
                set_err.set(None);
                set_step.set(MigrationStep::Previewing);
                spawn_local(async move {
                    match preview_migration(db_id, sid, id.clone()).await {
                        Ok(p)  => set_step.set(MigrationStep::Preview(p, sid, id)),
                        Err(e) => {
                            set_err.set(Some(e.to_string()));
                            set_step.set(MigrationStep::Search);
                        }
                    }
                });
            }
        >
            <div class="cover">
                {match cover {
                    Some(url) => view! { <img src=url alt=title.clone() /> }.into_any(),
                    None      => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                }}
            </div>
            <p>{title}</p>
        </div>
    }
}