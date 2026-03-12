use crate::server_fns::{
    cancel_download, check_in_library, delete_downloaded, delete_manga, download_all,
    download_chapter, fetch_sources, get_chapter_list, get_local_chapter_list, get_local_manga,
    get_manga_details, proxy_url, refresh_manga, save_to_library, scan_for_new_chapters,
    toggle_auto_download, get_categories, get_manga_categories, set_manga_categories,
    get_download_rules, add_download_rule, remove_download_rule
};
use crate::types::{ChapterList, LiveChapterStatus, Category, DownloadRule, DownloadRuleKind};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

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
                let sources = fetch_sources().await.unwrap_or_default();
                let source = sources.into_iter().find(|s| s.id == sid)
                    .ok_or_else(|| ServerFnError::new("Source not found"))?;
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

    // Library-only secondary resources — live at component level so they don't
    // recreate on every manga render. Keyed on db_id so they refetch on navigation.
    let did_signal        = move || db_id().unwrap_or_default();
    let categories_resource  = Resource::new(|| (), move |_| get_categories());
    let manga_cats_resource  = Resource::new(did_signal, get_manga_categories);
    let rules_resource       = Resource::new(did_signal, get_download_rules);

    Effect::new(move |_| {
        if let Some(Ok((_, _, _, _, ad, _))) = manga.get() {
            set_auto_download.set(ad);
        }
    });

    view! {
        <div class="manga-details">
            // 1. Wrap the resource reads in Suspense
            <Suspense fallback=move || view! {
                // 2. Move your skeleton loader here!
                <div class="manga-skeleton">
                    <div class="manga-skeleton__hero">
                        <div class="manga-skeleton__cover"></div>
                        <div class="manga-skeleton__meta">
                            <div class="manga-skeleton__title"></div>
                            <div class="manga-skeleton__lines">
                                <div class="manga-skeleton__line manga-skeleton__line--xs"></div>
                                <div class="manga-skeleton__line manga-skeleton__line--short"></div>
                                <div class="manga-skeleton__line manga-skeleton__line--short"></div>
                            </div>
                            <div class="manga-skeleton__lines">
                                <div class="manga-skeleton__line manga-skeleton__line--full"></div>
                                <div class="manga-skeleton__line manga-skeleton__line--full"></div>
                                <div class="manga-skeleton__line manga-skeleton__line--long"></div>
                                <div class="manga-skeleton__line manga-skeleton__line--mid"></div>
                            </div>
                        </div>
                    </div>
                    <div class="manga-skeleton__chapters">
                        {(0..7).map(|_| view! {
                            <div class="manga-skeleton__chapter-row"></div>
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            }>
                {move || {
                    // 3. The match no longer needs to handle None manually.
                    // If either resource is None, Suspense intercepts it and shows the fallback.
                    match (manga.get(), chapters.get()) {

                        // ── Manga load error ──────────────────────────────────────────
                        (Some(Err(e)), _) => view! {
                            <p class="error">"Error loading manga: " {e.to_string()}</p>
                        }.into_any(),

                        // ── Both resources ready ──────────────────────────────────────
                        (Some(Ok((info, source, initial_db_id, is_local_route, _auto_download_init, auto_scan))),
                         Some(chapter_result)) => {
                            let current_db_id    = move || added_db_id.get().or(initial_db_id);
                            let sid              = source.id;
                            let mid              = info.id.clone();
                            let base_url         = source.base_url.clone();
                            let info_title       = info.title.clone();
                            let info_cover       = info.cover_url.clone();
                            let info_status      = info.status.to_string();
                            let info_authors     = info.authors.clone();
                            let info_artists     = info.artists.clone();
                            let info_tags        = info.tags.clone();
                            let info_description = info.description.clone();
                            let did_val          = db_id().unwrap_or_default();

                            view! {
                                // ── Hero row: cover + scrollable metadata ─────────────────
                                <div class="manga-hero">

                                    <div class="manga-hero__cover">
                                        {match info_cover {
                                            Some(url) => {
                                                let src = proxy_url(&url, &base_url);
                                                view! { <img src=src alt=info_title /> }.into_any()
                                            }
                                            None => view! {
                                                <div class="no-cover">"No Cover"</div>
                                            }.into_any(),
                                        }}
                                    </div>

                                    <div class="manga-hero__meta">
                                        <h1>{info.title.clone()}</h1>

                                        // Status + authors/artists
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

                                        // Description — now adjacent to authors, easy to read
                                        <div class="description">
                                            <p>{info_description}</p>
                                        </div>

                                        // Genre / tag chips
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

                                        // Library action buttons
                                        <div class="library-actions">
                                            {move || {
                                                if is_local_route {
                                                    view! {
                                                        <button class="migrate-button">"Migrate"</button>
                                                        <button class="remove-button" on:click=move |_| {
                                                            leptos::task::spawn_local(async move {
                                                                if delete_manga(did_val).await.is_ok() {
                                                                    let navigate = leptos_router::hooks::use_navigate();
                                                                    navigate("/", Default::default());
                                                                }
                                                            });
                                                        }>"Remove from library"</button>
                                                        <button class="download-all-button" on:click=move |_| {
                                                            leptos::task::spawn_local(async move {
                                                                let _ = download_all(did_val).await;
                                                            });
                                                        }>"Download All"</button>
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
                                                        {move || scan_message.get().map(|msg| view! {
                                                            <span class="scan-message">{msg}</span>
                                                        }.into_any())}
                                                        {move || if auto_scan {
                                                            view! {
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
                                                    let m_clone = mid.clone();
                                                    view! {
                                                        <button
                                                            class="add-to-library"
                                                            disabled=library_pending
                                                            on:click=move |_| {
                                                                let m = m_clone.clone();
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
                                                    }.into_any()
                                                }
                                            }}
                                        </div>

                                        // ── Category chips (local manga only) ─────────────
                                        <Show when=move || is_local_route fallback=|| ()>
                                            <div class="category-selector">
                                                <p class="category-selector__label">"Categories"</p>
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
                                                            <div class="chip-group">
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
                                                                {move || if val.is_empty() {
                                                                    view! {
                                                                        <span class="category-selector__empty">
                                                                            "No categories yet — create some in Settings."
                                                                        </span>
                                                                    }.into_any()
                                                                } else {
                                                                    ().into_any()
                                                                }}
                                                            </div>
                                                        }
                                                    }}
                                                </Suspense>
                                            </div>
                                        </Show>

                                        // ── Download rules (local manga only) ─────────────
                                        <Show when=move || is_local_route fallback=|| ()>
                                            <div class="download-rules-panel">
                                                <p class="download-rules-panel__label">"Download Rules"</p>
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
                                            </div>
                                        </Show>

                                    </div> // end .manga-hero__meta
                                </div> // end .manga-hero

                                // ── Chapter list panel ────────────────────────────────────
                                <div class="chapter-list-group">
                                    <div class="chapter-list-header">
                                        <h2>"Chapters"</h2>
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
                                                                <div class="chapter-item">
                                                                    <div class="chapter-details">
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
                                                                        <div class="chapter-meta">
                                                                            <span class="chapter-scanlator">{chapter.scanlator.unwrap_or_default()}</span>
                                                                            <span class="chapter-date">
                                                                                {chapter.date_uploaded.map(|epoch| {
                                                                                    use chrono::DateTime;
                                                                                    DateTime::from_timestamp(epoch, 0)
                                                                                        .map(|dt| dt.format("%b %d, %Y").to_string())
                                                                                        .unwrap_or_default()
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
                                                                                                <button class="delete-button" on:click=move |_| {
                                                                                                    leptos::task::spawn_local(async move {
                                                                                                        if delete_downloaded(db_chap_id).await.is_ok() {
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
                                                                                                    });
                                                                                                }>"Delete"</button>
                                                                                            }.into_any(),

                                                                                            1 => {
                                                                                                let text = if let Some(p) = live {
                                                                                                    if p.total_pages > 0 {
                                                                                                        format!("Downloading... ({}/{})", p.completed_pages, p.total_pages)
                                                                                                    } else {
                                                                                                        "Downloading...".to_string()
                                                                                                    }
                                                                                                } else {
                                                                                                    "Downloading...".to_string()
                                                                                                };
                                                                                                view! {
                                                                                                    <button
                                                                                                        class="download-button download-button--active"
                                                                                                        on:click=move |_| {
                                                                                                            leptos::task::spawn_local(async move {
                                                                                                                chapters_progress.update(|m| { m.remove(&db_chap_id); });
                                                                                                                let _ = cancel_download(db_chap_id).await;
                                                                                                            });
                                                                                                        }
                                                                                                    >{text}</button>
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

                                                                                            _ => view! {
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
                                                                                            }.into_any()
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
                                                        <div class="pagination">
                                                            <button
                                                                on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1))
                                                                disabled=move || page.get() <= 1
                                                            >"Prev"</button>
                                                            <span>" Page " {page} </span>
                                                            <button
                                                                on:click=move |_| set_page.update(|p| *p += 1)
                                                                disabled=move || !list.has_next_page
                                                            >"Next"</button>
                                                        </div>
                                                    </Show>
                                                }.into_any()
                                            }
                                        }}
                                    </div> // end .chapter-list
                                </div> // end .chapter-list-group
                            }.into_any()
                        },
                        _ => ().into_any()
                    }
                }}
            </Suspense>
        </div>
    }
}
