use crate::pages::components::collapsible_panel::CollapsiblePanel;
use crate::pages::components::pagination::Pagination;
use crate::pages::components::migration_dialogue::MigrationDialogue;
use crate::pages::components::permission_handlers::PermissionGate;
use crate::server_fns::{
    add_download_rule, cancel_download, check_in_library, delete_downloaded, delete_manga, 
    download_all, download_chapter, get_categories, get_chapter_list, get_download_rules, 
    get_local_chapter_list, get_local_manga, get_manga_categories, get_manga_details, 
    get_scanlator_preferences, get_source, refresh_manga, remove_download_rule, 
    remove_scanlator_preference, save_to_library, scan_for_new_chapters, set_manga_categories, 
    set_scanlator_preference, toggle_auto_download, global_search,
};
use crate::types::{
    Category, DownloadRule, DownloadRuleKind, LiveChapterStatus, MigrationStep, SearchScope
};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use time::macros::format_description;

#[allow(non_snake_case)]
#[component]
pub fn MangaDetails() -> impl IntoView {
    let params = use_params_map();

    let source_id = move || params.with(|p| p.get("id").unwrap_or_default().parse::<i64>().ok());
    let manga_id  = move || params.with(|p| p.get("manga_id"));
    let db_id     = move || params.with(|p| p.get("db_id").unwrap_or_default().parse::<i64>().ok());

    let (page, set_page)                     = signal(1);
    let (added_db_id, set_added_db_id)       = signal::<Option<i64>>(None);
    let (sort_order_sig, set_sort_order)     = signal(crate::types::ChapterSortOrder::default());
    let (library_pending, set_library_pending) = signal(false);
    let (refreshing, set_refreshing)         = signal(false);
    let (scanning, set_scanning)             = signal(false);
    let (scan_message, set_scan_message)     = signal(None::<String>);
    let (auto_download_sig, set_auto_download) = signal(false);
    let (cat_pending, set_cat_pending)       = signal(false);
    let (new_rule_type, set_new_rule_type)   = signal("scanlator_include".to_string());
    let (new_rule_value, set_new_rule_value) = signal(String::new());
    let (rule_error, set_rule_error)         = signal(Option::<String>::None);
    let (migration_step, set_migration_step) = signal(MigrationStep::Closed);
    let (mig_scope, set_mig_scope)           = signal(SearchScope::FavouritedOnly);
    let (mig_query, set_mig_query)           = signal(String::new());
    let (mig_error, set_mig_error)           = signal(Option::<String>::None);  

    let chapters_progress = expect_context::<RwSignal<std::collections::HashMap<i64, crate::types::ChapterProgress>>>();

    let is_local = move || db_id().is_some();

    let manga = Resource::new(
        move || (source_id(), manga_id(), db_id()),
        move |(sid, mid, did)| async move {
            if let Some(did) = did {
                let (info, source, auto_download, auto_scan) = get_local_manga(did).await?;
                Ok((info, source, Some(did), true, auto_download, auto_scan))
            } else if let (Some(sid), Some(mid)) = (sid, mid) {
                let info = get_manga_details(sid, mid.clone()).await?;
                let source = get_source(sid).await?;
                let existing_db_id = check_in_library(sid, mid).await?;
                Ok((info, source, existing_db_id, false, false, false))
            } else {
                Err(ServerFnError::new("Invalid route parameters"))
            }
        },
    );

    let chapters = Resource::new(
        move || (source_id(), manga_id(), db_id(), page.get(), sort_order_sig.get()),
        move |(sid, mid, did, p, sort_order)| async move {
            if let Some(did) = did {
                get_local_chapter_list(did, p, sort_order).await
            } else if let (Some(sid), Some(mid)) = (sid, mid) {
                get_chapter_list(sid, mid, p).await
            } else {
                Err(ServerFnError::new("Invalid route parameters"))
            }
        },
    );

    let mig_query_debounced = crate::utils::use_debounced_signal(mig_query, 400);

    let migration_search_results = Resource::new(
        move || (mig_scope.get(), mig_query_debounced.get()),
        move |(scope, q)| async move {
            if q.is_empty() {
                return Ok(vec![]);
            }
            global_search(q, scope, 1).await
        },
    );

    let did_signal        = move || db_id().unwrap_or_default();
    let categories_resource  = Resource::new(|| (), move |_| get_categories());
    let manga_cats_resource  = Resource::new(did_signal, get_manga_categories);
    let rules_resource       = Resource::new(did_signal, get_download_rules);
    let scanlator_prefs_resource = Resource::new(did_signal, get_scanlator_preferences);

    Effect::new(move |_| {
        if let Some(Ok((_, _, _, _, ad, _))) = manga.get() {
            set_auto_download.set(ad);
        }
    });

    let fmt = format_description!("[month repr:short] [day], [year]");

    view! {
        <div class="manga-details">
            <Suspense fallback=move || view! {
                <div class="skeleton-manga-hero">
                    <div class="skeleton-manga-hero__cover"></div>
                    <div class="skeleton-manga-hero__meta">
                        <div class="skeleton-manga-hero__title"></div>
                        <div class="skeleton-manga-hero__lines">
                            <div class="skeleton-row skeleton-row--xs" style="width: 20%"></div>
                            <div class="skeleton-row skeleton-row--xs" style="width: 35%"></div>
                        </div>
                        <div class="skeleton-manga-hero__lines">
                            <div class="skeleton-row skeleton-row--xs" style="width: 100%"></div>
                            <div class="skeleton-row skeleton-row--xs" style="width: 100%"></div>
                            <div class="skeleton-row skeleton-row--xs" style="width: 75%"></div>
                            <div class="skeleton-row skeleton-row--xs" style="width: 55%"></div>
                        </div>
                    </div>
                </div>

                <div class="chapter-list-group">
                    <div class="skeleton-manga-hero__lines" style="margin-bottom: var(--sp-3)">
                        <div class="skeleton-row skeleton-row--xs" style="width: 100px"></div>
                    </div>
                    <div class="skeleton-list">
                        {(0..7).map(|_| view! {
                            <div class="skeleton-row"></div>
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            }>
                {move || {
                    match (manga.get(), chapters.get()) {
                        (Some(Err(e)), _) => view! {
                            <p class="error">"Error loading manga: " {e.to_string()}</p>
                        }.into_any(),

                        (Some(Ok((info, source, initial_db_id, is_local_route, _auto_download_init, auto_scan))),
                         Some(chapter_result)) => {
                            let current_db_id    = move || added_db_id.get().or(initial_db_id);
                            let sid              = source.id;
                            let mid              = info.id.clone();
                            let info_title       = info.title.clone();
                            let migrate_title    = info_title.clone();
                            let info_cover       = info.cover_url.clone();
                            let info_status      = info.status.to_string();
                            let info_authors     = info.authors.clone();
                            let info_artists     = info.artists.clone();
                            let info_tags        = info.tags.clone();
                            let info_description = info.description.clone();
                            let info_description_html = info.description_html.clone();
                            let did_val          = db_id().unwrap_or_default();

                            view! {
                                <div class="manga-hero">
                                    <div class="manga-hero__cover">
                                        {match info_cover {
                                            Some(url) => {
                                                view! { <img src=url alt=info_title /> }.into_any()
                                            }
                                            None => view! {
                                                <div class="no-cover">"No Cover"</div>
                                            }.into_any(),
                                        }}
                                    </div>

                                    <div class="manga-hero__meta">
                                        <h1>{info.title.clone()}</h1>

                                        <div class="details">
                                            <div class="status">
                                                <p>"Status: " {info_status}</p>
                                            </div>
                                            <div class="people">
                                                <div class="authors">
                                                    <p>"Author: "</p>
                                                    <For
                                                        each=move || info_authors.clone()
                                                        key=|author: &String| author.clone()
                                                        children=move |author: String| view! {
                                                            <div class="author">
                                                                <A href=format!("/?author={}", author)>{author}</A>
                                                            </div>
                                                        }
                                                    />
                                                </div>
                                                <div class="artists">
                                                    <p>"Artist: "</p>
                                                    <For
                                                        each=move || info_artists.clone()
                                                        key=|artist: &String| artist.clone()
                                                        children=move |artist: String| view! {
                                                            <div class="artist">
                                                                <A href=format!("/?artist={}", artist)>{artist}</A>
                                                            </div>
                                                        }
                                                    />
                                                </div>
                                            </div>
                                        </div>

                                        <div class="description"
                                            inner_html=info_description_html
                                                .or(info_description.map(|d| format!("<p>{d}</p>")))
                                                .unwrap_or_default()
                                        />

                                        <div class="tags">
                                            <For
                                                each=move || info_tags.clone()
                                                key=|tag: &String| tag.clone()
                                                children=move |tag: String| view! {
                                                    <div class="tag">
                                                        <A href=format!("/?tag={}", tag)>{tag}</A>
                                                    </div>
                                                }
                                            />
                                        </div>

                                        <div class="library-actions">
                                            {move || {
                                                let migrate_title = migrate_title.clone();
                                                if is_local_route {
                                                    let migrate_title = StoredValue::new(migrate_title);
                                                    view! {
                                                        <PermissionGate permission="library:manage">
                                                            <button class="migrate-button" on:click=move |_| {
                                                                set_mig_query.set(migrate_title.get_value());
                                                                set_mig_error.set(None);
                                                                set_migration_step.set(MigrationStep::Search);
                                                            }>
                                                                "Migrate"
                                                            </button>
                                                        </PermissionGate>
                                                        <PermissionGate permission="library:delete">
                                                            <button class="remove-button" on:click=move |_| {
                                                                leptos::task::spawn_local(async move {
                                                                    if delete_manga(did_val).await.is_ok() {
                                                                        let navigate = leptos_router::hooks::use_navigate();
                                                                        navigate("/", Default::default());
                                                                    }
                                                                });
                                                            }>"Remove from library"</button>
                                                        </PermissionGate>
                                                        <PermissionGate permission="chapter:download">
                                                            <button class="download-all-button" on:click=move |_| {
                                                                leptos::task::spawn_local(async move {
                                                                    let _ = download_all(did_val).await;
                                                                });
                                                            }>"Download All"</button>
                                                        </PermissionGate>
                                                        <PermissionGate permission="library:refresh">
                                                            <button
                                                                class="refresh-button"
                                                                disabled=move || refreshing.get()
                                                                on:click=move |_| {
                                                                    if refreshing.get() { return; }
                                                                    set_refreshing.set(true);
                                                                    leptos::task::spawn_local(async move {
                                                                        let _ = refresh_manga(did_val).await;
                                                                        set_refreshing.set(false);
                                                                    });
                                                                }
                                                            >
                                                                {move || if refreshing.get() { "Refreshing..." } else { "Refresh" }}
                                                            </button>
                                                        </PermissionGate>
                                                        <PermissionGate permission="library:refresh">
                                                            <button
                                                                class="scan-button"
                                                                disabled=move || scanning.get()
                                                                on:click=move |_| {
                                                                    if scanning.get() { return; }
                                                                    set_scanning.set(true);
                                                                    set_scan_message.set(None);
                                                                    leptos::task::spawn_local(async move {
                                                                        match scan_for_new_chapters(did_val).await {
                                                                            Ok(cnt) if cnt > 0 => {
                                                                                set_scan_message.set(Some(format!("Found {} new chapters!", cnt)));
                                                                                chapters.refetch();
                                                                            }
                                                                            Ok(_) => set_scan_message.set(Some("No new chapters found.".to_string())),
                                                                            Err(e) => set_scan_message.set(Some(format!("Scan failed: {:?}", e))),
                                                                        }
                                                                        set_scanning.set(false);
                                                                    });
                                                                }
                                                            >
                                                                {move || if scanning.get() { "Scanning..." } else { "Scan for new chapters" }}
                                                            </button>
                                                        </PermissionGate>
                                                        {move || scan_message.get().map(|msg| view! {
                                                            <span class="scan-message">{msg}</span>
                                                        }.into_any())}
                                                        {move || if auto_scan {
                                                            view! {
                                                                <PermissionGate permission="library:manage">
                                                                    <div class="auto-download-toggle">
                                                                        <label>
                                                                            <input
                                                                                type="checkbox"
                                                                                checked=move || auto_download_sig.get()
                                                                                on:change=move |ev| {
                                                                                    let checked = event_target_checked(&ev);
                                                                                    set_auto_download.set(checked);
                                                                                    leptos::task::spawn_local(async move {
                                                                                        let _ = toggle_auto_download(did_val, checked).await;
                                                                                    });
                                                                                }
                                                                            />
                                                                            " Auto-Download New Chapters"
                                                                        </label>
                                                                    </div>
                                                                </PermissionGate>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span/> }.into_any()
                                                        }}
                                                    }.into_any()
                                                } else if let Some(id) = current_db_id() {
                                                    view! {
                                                        <A href=format!("/manga/{}", id)>
                                                            <button class="go-to-library-button">"Go to Library"</button>
                                                        </A>
                                                    }.into_any()
                                                } else {
                                                    let m_sv = StoredValue::new(mid.clone());
                                                    view! {
                                                        <PermissionGate permission="library:add">
                                                            <button
                                                                class="add-to-library"
                                                                disabled=library_pending
                                                                on:click=move |_| {
                                                                    let m = m_sv.get_value();
                                                                    set_library_pending.set(true);
                                                                    leptos::task::spawn_local(async move {
                                                                        if let Ok(new_id) = save_to_library(sid, m).await {
                                                                            set_added_db_id.set(Some(new_id));
                                                                        }
                                                                        set_library_pending.set(false);
                                                                    });
                                                                }
                                                            >
                                                                {move || if library_pending.get() { "Saving..." } else { "Add to Library" }}
                                                            </button>
                                                        </PermissionGate>
                                                    }.into_any()
                                                }
                                            }}
                                        </div>

                                        <PermissionGate permission="library:manage">
                                        <Show when=move || is_local_route fallback=|| ()>
                                            <CollapsiblePanel label="Categories".to_string()>
                                                <Suspense fallback=|| ()>
                                                    {move || {
                                                        let all: Vec<Category> = categories_resource.get()
                                                            .and_then(|r| r.ok())
                                                            .unwrap_or_default();
                                                        let val = all.clone();
                                                        let assigned: Vec<i64> = manga_cats_resource.get()
                                                            .and_then(|r| r.ok())
                                                            .unwrap_or_default()
                                                            .iter()
                                                            .map(|c: &Category| c.id)
                                                            .collect();

                                                        view! {
                                                            <div class="category-selector__chips">
                                                                <For
                                                                    each=move || all.clone()
                                                                    key=|c: &Category| c.id
                                                                    children=move |cat| {
                                                                        let cat_id         = cat.id;
                                                                        let is_on          = assigned.contains(&cat_id);
                                                                        let cls            = if is_on { "chip chip--active" } else { "chip" };
                                                                        let assigned_clone = assigned.clone();

                                                                        view! {
                                                                            <button
                                                                                class=cls
                                                                                disabled=move || cat_pending.get()
                                                                                on:click=move |_| {
                                                                                    let mut next = assigned_clone.clone();
                                                                                    if next.contains(&cat_id) {
                                                                                        next.retain(|&x| x != cat_id);
                                                                                    } else {
                                                                                        next.push(cat_id);
                                                                                    }
                                                                                    set_cat_pending.set(true);
                                                                                    leptos::task::spawn_local(async move {
                                                                                        let _ = set_manga_categories(did_val, next).await;
                                                                                        manga_cats_resource.refetch();
                                                                                        set_cat_pending.set(false);
                                                                                    });
                                                                                }
                                                                            >
                                                                                {cat.name.clone()}
                                                                            </button>
                                                                        }
                                                                    }
                                                                />
                                                            </div>
                                                            {move || if val.is_empty() {
                                                                view! {
                                                                    <span class="category-selector__empty">
                                                                        "No categories yet — create some in Settings."
                                                                    </span>
                                                                }.into_any()
                                                            } else {
                                                                ().into_any()
                                                            }}
                                                        }
                                                    }}
                                                </Suspense>
                                            </CollapsiblePanel>
                                        </Show>
                                        </PermissionGate>

                                        <PermissionGate permission="library:manage">
                                        <Show when=move || is_local_route fallback=|| ()>
                                            <CollapsiblePanel label="Download Rules".to_string()>
                                                <p class="collapsible-panel__hint">
                                                    "Filter which chapters are automatically downloaded based on scanlator, language, or title."
                                                </p>
                                                <Suspense fallback=|| ()>
                                                    {move || {
                                                        let rules: Vec<DownloadRule> = rules_resource.get()
                                                            .and_then(|r| r.ok())
                                                            .unwrap_or_default();
                                                        view! {
                                                            <ul class="rule-list">
                                                                <For
                                                                    each=move || rules.clone()
                                                                    key=|r: &DownloadRule| r.id
                                                                    children=move |rule| {
                                                                        let rule_id = rule.id;
                                                                        view! {
                                                                            <li class="rule-list__item">
                                                                                <span class="rule-list__label">{rule.kind.to_string()}</span>
                                                                                <button
                                                                                    class="rule-list__remove"
                                                                                    title="Remove rule"
                                                                                    on:click=move |_| {
                                                                                        leptos::task::spawn_local(async move {
                                                                                            let _ = remove_download_rule(rule_id).await;
                                                                                            rules_resource.refetch();
                                                                                        });
                                                                                    }
                                                                                >"×"</button>
                                                                            </li>
                                                                        }
                                                                    }
                                                                />
                                                            </ul>
                                                            <div class="rule-add-row">
                                                                <select
                                                                    on:change=move |ev| set_new_rule_type.set(event_target_value(&ev))
                                                                >
                                                                    <option value="scanlator_include">"Scanlator — include"</option>
                                                                    <option value="scanlator_exclude">"Scanlator — exclude"</option>
                                                                    <option value="language_include">"Language — include"</option>
                                                                    <option value="language_exclude">"Language — exclude"</option>
                                                                    <option value="title_contains">"Title — contains"</option>
                                                                    <option value="title_excludes">"Title — excludes"</option>
                                                                </select>
                                                                <input
                                                                    type="text"
                                                                    placeholder="Value…"
                                                                    prop:value=move || new_rule_value.get()
                                                                    on:input=move |ev| set_new_rule_value.set(event_target_value(&ev))
                                                                />
                                                                <button
                                                                    class="rule-add-btn"
                                                                    on:click=move |_| {
                                                                        let val  = new_rule_value.get_untracked();
                                                                        let kind = match new_rule_type.get_untracked().as_str() {
                                                                            "scanlator_include" => DownloadRuleKind::ScanlatorInclude(val),
                                                                            "scanlator_exclude" => DownloadRuleKind::ScanlatorExclude(val),
                                                                            "language_include"  => DownloadRuleKind::LanguageInclude(val),
                                                                            "language_exclude"  => DownloadRuleKind::LanguageExclude(val),
                                                                            "title_contains"    => DownloadRuleKind::TitleContains(val),
                                                                            _                   => DownloadRuleKind::TitleExcludes(val),
                                                                        };
                                                                        leptos::task::spawn_local(async move {
                                                                            match add_download_rule(did_val, kind).await {
                                                                                Ok(_) => {
                                                                                    set_new_rule_value.set(String::new());
                                                                                    set_rule_error.set(None);
                                                                                    rules_resource.refetch();
                                                                                }
                                                                                Err(e) => set_rule_error.set(Some(e.to_string())),
                                                                            }
                                                                        });
                                                                    }
                                                                >"+ Add Rule"</button>
                                                            </div>
                                                            {move || rule_error.get().map(|e| view! {
                                                                <p class="error">{e}</p>
                                                            })}
                                                        }
                                                    }}
                                                </Suspense>
                                            </CollapsiblePanel>
                                        </Show>
                                        </PermissionGate>

                                        <PermissionGate permission="library:manage">
                                        <Show when=move || is_local_route fallback=|| ()>
                                            <CollapsiblePanel label="Scanlator Preferences".to_string()>
                                                <p class="collapsible-panel__hint">
                                                    "Set priority for scanlation groups. When the same chapter number "
                                                    "is uploaded by multiple groups, the highest-priority group is "
                                                    "shown first and auto-downloaded."
                                                </p>

                                                <Suspense fallback=|| ()>
                                                    {move || {
                                                        let known_scanlators: Vec<String> = chapters.get()
                                                            .and_then(|r| r.ok())
                                                            .map(|cl| {
                                                                let mut seen = std::collections::HashSet::new();
                                                                cl.chapters.into_iter()
                                                                    .filter_map(|c| c.scanlator)
                                                                    .filter(|s| !s.is_empty() && seen.insert(s.clone()))
                                                                    .collect()
                                                            })
                                                            .unwrap_or_default();

                                                        let prefs = scanlator_prefs_resource.get()
                                                            .and_then(|r| r.ok())
                                                            .unwrap_or_default();

                                                        let already_set: std::collections::HashSet<String> = prefs.iter()
                                                            .map(|p| p.scanlator.clone())
                                                            .collect();
                                                        let unset: Vec<String> = known_scanlators.into_iter()
                                                            .filter(|s| !already_set.contains(s))
                                                            .collect();

                                                        view! {
                                                            <ul class="scanlator-pref-list">
                                                                <For
                                                                    each=move || prefs.clone()
                                                                    key=|p| p.id
                                                                    children=move |pref| {
                                                                        let pref_id = pref.id;
                                                                        let (local_prio, set_local_prio) = signal(pref.priority);
                                                                        view! {
                                                                            <li class="scanlator-pref-item">
                                                                                <span class="scanlator-pref-item__name">
                                                                                    {pref.scanlator.clone()}
                                                                                </span>
                                                                                <div class="scanlator-pref-item__controls">
                                                                                    <input
                                                                                        type="number"
                                                                                        class="scanlator-pref-item__priority"
                                                                                        min="-100"
                                                                                        max="100"
                                                                                        prop:value=move || local_prio.get().to_string()
                                                                                        on:change=move |ev| {
                                                                                            if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                                                                                set_local_prio.set(v);
                                                                                                let scanlator = pref.scanlator.clone();
                                                                                                leptos::task::spawn_local(async move {
                                                                                                    let _ = set_scanlator_preference(did_val, scanlator, v).await;
                                                                                                    scanlator_prefs_resource.refetch();
                                                                                                });
                                                                                            }
                                                                                        }
                                                                                    />
                                                                                    <button
                                                                                        class="rule-list__remove"
                                                                                        title="Remove preference"
                                                                                        on:click=move |_| {
                                                                                            leptos::task::spawn_local(async move {
                                                                                                let _ = remove_scanlator_preference(pref_id).await;
                                                                                                scanlator_prefs_resource.refetch();
                                                                                            });
                                                                                        }
                                                                                    >"×"</button>
                                                                                </div>
                                                                            </li>
                                                                        }
                                                                    }
                                                                />
                                                            </ul>

                                                            {
                                                                if unset.is_empty() {
                                                                    view! { <span></span> }.into_any()
                                                                } else {
                                                                    let (sel, set_sel) = signal(unset.first().cloned().unwrap_or_default());
                                                                    let unset_clone = unset.clone();
                                                                    view! {
                                                                        <div class="rule-add-row">
                                                                            <select on:change=move |ev| set_sel.set(event_target_value(&ev))>
                                                                                <For
                                                                                    each=move || unset_clone.clone()
                                                                                    key=|s| s.clone()
                                                                                    children=|s| view! {
                                                                                        <option value=s.clone()>{s.clone()}</option>
                                                                                    }
                                                                                />
                                                                            </select>
                                                                            <button
                                                                                class="rule-add-btn"
                                                                                on:click=move |_| {
                                                                                    let scanlator = sel.get_untracked();
                                                                                    leptos::task::spawn_local(async move {
                                                                                        let _ = set_scanlator_preference(did_val, scanlator, 0).await;
                                                                                        scanlator_prefs_resource.refetch();
                                                                                    });
                                                                                }
                                                                            >"+ Add preference"</button>
                                                                        </div>
                                                                    }.into_any()
                                                                }
                                                            }
                                                        }
                                                    }}
                                                </Suspense>
                                            </CollapsiblePanel>
                                        </Show>
                                        </PermissionGate>
                                    </div>
                                </div>

                                <div class="chapter-list-group">
                                    <div class="chapter-list-header">
                                        <h2>"Chapters"</h2>
                                        <PermissionGate permission="library:manage">
                                        <Show when=move || is_local() fallback=|| ()>
                                            <select
                                                prop:value=move || sort_order_sig.get().to_select_value()
                                                on:change=move |ev| {
                                                    let val = event_target_value(&ev);
                                                    set_sort_order.set(crate::types::ChapterSortOrder::from_select_value(&val));
                                                    set_page.set(1);
                                                }
                                            >
                                                <option value="uploaded_desc">"Newest first"</option>
                                                <option value="uploaded_asc">"Oldest first"</option>
                                                <option value="chapter_desc">"Ch. # ↓"</option>
                                                <option value="chapter_asc">"Ch. # ↑"</option>
                                                <option value="volume_desc">"Volume ↓"</option>
                                                <option value="volume_asc">"Volume ↑"</option>
                                                <option value="language_asc">"Language A→Z"</option>
                                                <option value="language_desc">"Language Z→A"</option>
                                                <option value="scanlator_asc">"Scanlator A→Z"</option>
                                                <option value="scanlator_desc">"Scanlator Z→A"</option>
                                            </select>
                                        </Show>
                                        </PermissionGate>
                                    </div>

                                    <div class="chapter-list">
                                        {match chapter_result {
                                            Err(e) => view! {
                                                <p class="error">"Error loading chapters: " {e.to_string()}</p>
                                            }.into_any(),
                                            Ok(list) => {
                                                let list_chapters   = list.clone();
                                                let list_pagination = list.clone();
                                                view! {
                                                    <For
                                                        each=move || list_chapters.chapters.clone()
                                                        key=|chapter| chapter.id.clone()
                                                        children=move |chapter| {
                                                            let chap_id_str      = chapter.id.clone();
                                                            view! {
                                                                <div class=if chapter.is_orphaned { "chapter-item chapter-item--orphaned" } else { "chapter-item" }>
                                                                    <div class="chapter-details">
                                                                        <div class="chapter-title-container">
                                                                            <span class="chapter-title">
                                                                                {{
                                                                                    let mut title_str = String::new();
                                                                                    if let Some(vol) = &chapter.volume {
                                                                                        title_str.push_str(&format!("Vol. {} ", vol));
                                                                                    }
                                                                                    title_str.push_str(&format!("Ch. {}", chapter.number));
                                                                                    if let Some(title) = &chapter.title
                                                                                        && !title.is_empty() {
                                                                                            title_str.push_str(&format!(" - {}", title));
                                                                                        }
                                                                                    title_str
                                                                                }}
                                                                            </span>
                                                                            {if chapter.is_orphaned {
                                                                                view! {
                                                                                    <span class="chapter-orphaned-badge">"Orphaned"</span>
                                                                                }.into_any()
                                                                            } else {
                                                                                view! { <span/> }.into_any()
                                                                            }}
                                                                        </div>
                                                                        <div class="chapter-meta">
                                                                            <span class="chapter-scanlator">{chapter.scanlator.unwrap_or_default()}</span>
                                                                            <span class="chapter-date">
                                                                                {chapter.date_uploaded.map(|epoch| {
                                                                                    time::OffsetDateTime::from_unix_timestamp(epoch)
                                                                                        .ok()
                                                                                        .map(|dt| dt.format(fmt).unwrap_or_default())
                                                                                }).unwrap_or_default()}
                                                                            </span>
                                                                        </div>
                                                                    </div>

                                                                    <div class="chapter-actions">
                                                                        <Show when=move || is_local() fallback=|| ()>
                                                                            {
                                                                                let cid              = chap_id_str.clone();
                                                                                let db_chap_id       = cid.parse::<i64>().unwrap_or(0);
                                                                                let is_downloaded    = chapter.download_status == 2;
                                                                                let is_downloading_db = chapter.download_status == 1;

                                                                                view! {
                                                                                    {move || {
                                                                                        let map  = chapters_progress.get();
                                                                                        let live = map.get(&db_chap_id);

                                                                                        let status = match live {
                                                                                            Some(p) => match &p.status {
                                                                                                LiveChapterStatus::InProgress => 1,
                                                                                                LiveChapterStatus::Completed
                                                                                                | LiveChapterStatus::CompletedHidden => 2,
                                                                                                LiveChapterStatus::Failed(_) => 3,
                                                                                                LiveChapterStatus::Cancelled
                                                                                                | LiveChapterStatus::Deleted => 0,
                                                                                            },
                                                                                            None => {
                                                                                                if is_downloaded { 2 }
                                                                                                else if is_downloading_db { 1 }
                                                                                                else { 0 }
                                                                                            }
                                                                                        };

                                                                                        match status {
                                                                                            2 => view! {
                                                                                                <PermissionGate permission="chapter:delete">
                                                                                                    <button class="delete-button" on:click=move |_| {
                                                                                                        leptos::task::spawn_local(async move {
                                                                                                            if delete_downloaded(db_chap_id).await.is_ok() {
                                                                                                                if chapter.is_orphaned {
                                                                                                                    chapters.refetch();
                                                                                                                } else {
                                                                                                                    chapters_progress.update(|m| {
                                                                                                                        m.insert(db_chap_id, crate::types::ChapterProgress {
                                                                                                                            id: db_chap_id,
                                                                                                                            name: String::new(),
                                                                                                                            total_pages: 0,
                                                                                                                            completed_pages: 0,
                                                                                                                            status: LiveChapterStatus::Deleted,
                                                                                                                        });
                                                                                                                    });
                                                                                                                }
                                                                                                            }
                                                                                                        });
                                                                                                    }>"Delete"</button>
                                                                                                </PermissionGate>
                                                                                            }.into_any(),

                                                                                            1 => {
                                                                                                let text = StoredValue::new(if let Some(p) = live {
                                                                                                    if p.total_pages > 0 {
                                                                                                        format!("Downloading... ({}/{})", p.completed_pages, p.total_pages)
                                                                                                    } else {
                                                                                                        "Downloading...".to_string()
                                                                                                    }
                                                                                                } else {
                                                                                                    "Downloading...".to_string()
                                                                                                });
                                                                                                view! {
                                                                                                    <PermissionGate permission="chapter:download">
                                                                                                        <button
                                                                                                            class="download-button download-button--active"
                                                                                                            on:click=move |_| {
                                                                                                                leptos::task::spawn_local(async move {
                                                                                                                    chapters_progress.update(|m| { m.remove(&db_chap_id); });
                                                                                                                    let _ = cancel_download(db_chap_id).await;
                                                                                                                });
                                                                                                            }
                                                                                                        >{text.get_value()}</button>
                                                                                                    </PermissionGate>
                                                                                                }.into_any()
                                                                                            }

                                                                                            3 => {
                                                                                                let msg = if let Some(crate::types::ChapterProgress {
                                                                                                    status: LiveChapterStatus::Failed(err), ..
                                                                                                }) = live {
                                                                                                    format!("Failed: {}", err)
                                                                                                } else {
                                                                                                    "Failed".to_string()
                                                                                                };
                                                                                                view! {
                                                                                                    <button
                                                                                                        class="download-button download-button--failed"
                                                                                                        disabled=true
                                                                                                    >{msg}</button>
                                                                                                }.into_any()
                                                                                            }

                                                                                            _ => if chapter.is_orphaned {
                                                                                                view! { <span/> }.into_any()
                                                                                            } else {
                                                                                                view! {
                                                                                                    <PermissionGate permission="chapter:download">
                                                                                                        <button
                                                                                                            class="download-button"
                                                                                                            on:click=move |_| {
                                                                                                                leptos::task::spawn_local(async move {
                                                                                                                    chapters_progress.update(|m| {
                                                                                                                        m.insert(db_chap_id, crate::types::ChapterProgress {
                                                                                                                            id: db_chap_id,
                                                                                                                            name: String::new(),
                                                                                                                            total_pages: 0,
                                                                                                                            completed_pages: 0,
                                                                                                                            status: LiveChapterStatus::InProgress,
                                                                                                                        });
                                                                                                                    });
                                                                                                                    let _ = download_chapter(db_chap_id).await;
                                                                                                                });
                                                                                                            }
                                                                                                        >"Download"</button>
                                                                                                    </PermissionGate>
                                                                                                }.into_any()
                                                                                            }
                                                                                        }
                                                                                    }}
                                                                                }
                                                                            }
                                                                        </Show>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }
                                                    />
                                                    <Show
                                                        when=move || !list_pagination.chapters.is_empty()
                                                        fallback=move || view! { <p class="empty">"No chapters found."</p> }
                                                    >
                                                        <Pagination page set_page has_next=Signal::derive(move || list.has_next_page) />
                                                    </Show>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                </div>
                                <MigrationDialogue
                                    db_id=did_val
                                    current_source_id=sid
                                    current_source_name=source.name.clone()
                                    current_title=info.title.clone()
                                    migration_step=migration_step
                                    set_migration_step=set_migration_step
                                    mig_scope=mig_scope
                                    set_mig_scope=set_mig_scope
                                    mig_query=mig_query
                                    set_mig_query=set_mig_query
                                    mig_error=mig_error
                                    set_mig_error=set_mig_error
                                    search_resource=migration_search_results
                                    on_complete=Callback::new(move |_| {
                                        manga.refetch();
                                        chapters.refetch();
                                    })
                                />
                            }.into_any()
                        },
                        _ => ().into_any()
                    }
                }}
            </Suspense>
        </div>
        
    }
}
