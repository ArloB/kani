use crate::pages::components::pagination::Pagination;
use crate::server_fns::{download_chapter, get_recent_updates};
use crate::types::{LiveChapterStatus, RecentUpdateItem};
use leptos::prelude::*;
use leptos_router::components::A;

type GroupedUpdates = Vec<(i64, String, Option<String>, Vec<RecentUpdateItem>)>;

// Group the flat list by manga_id while preserving first-seen order.
fn group_by_manga(updates: Vec<RecentUpdateItem>) -> GroupedUpdates {
    let mut seen: std::collections::HashMap<i64, usize> = Default::default();
    let mut groups: GroupedUpdates = Vec::new();

    for update in updates {
        let idx = if let Some(&i) = seen.get(&update.manga_id) {
            i
        } else {
            let i = groups.len();
            seen.insert(update.manga_id, i);
            groups.push((
                update.manga_id,
                update.manga_name.clone(),
                update.cover_url.clone(),
                Vec::new(),
            ));
            i
        };
        groups[idx].3.push(update);
    }
    groups
}

#[component]
pub fn RecentUpdates() -> impl IntoView {
    let (page, set_page) = signal(1i32);
    let chapters_progress = expect_context::<
        RwSignal<std::collections::HashMap<i64, crate::types::ChapterProgress>>,
    >();

    let updates = Resource::new(
        move || page.get(),
        |p| async move { get_recent_updates(p).await },
    );

    view! {
        <div class="recent-updates-page">
            <div class="page-header">
                <h1>"Recent Updates"</h1>
            </div>

            <Suspense fallback=move || view! { <p class="spinner">"Loading…"</p> }>
                {move || updates.get().map(|res| match res {
                    Err(e) => view! { <p class="error">"Error: "{e.to_string()}</p> }.into_any(),
                    Ok(list) if list.recent_updates.is_empty() => view! {
                        <p class="empty">"No recent updates yet. Add manga to your library and scan for chapters."</p>
                    }.into_any(),
                    Ok(list) => {
                        let groups = group_by_manga(list.recent_updates);
                        view! {
                            <div class="update-group-list">
                                <For
                                    each=move || groups.clone()
                                    key=|(manga_id, ..)| *manga_id
                                    children=move |(manga_id, manga_name, cover_url, chapters)| {
                                        view! {
                                            <div class="update-group">
                                                <div class="update-group__header">
                                                    <A href=format!("/manga/{}", manga_id)>
                                                        <div class="update-group__thumb">
                                                            {match cover_url {
                                                                Some(src) => view! {
                                                                    <img src=src alt=manga_name.clone() />
                                                                }.into_any(),
                                                                None => view! {
                                                                    <div class="no-cover">"?"</div>
                                                                }.into_any(),
                                                            }}
                                                        </div>
                                                        <span class="update-group__title">{manga_name.clone()}</span>
                                                    </A>
                                                </div>

                                                <ul class="update-chapter-list">
                                                    <For
                                                        each=move || chapters.clone()
                                                        key=|ch| ch.chapter_id
                                                        children=move |ch| {
                                                            let chapter_id = ch.chapter_id;
                                                            let label = {
                                                                let mut s = format!("Ch. {}", ch.chapter_number);
                                                                if let Some(t) = &ch.chapter_name
                                                                    && !t.is_empty()
                                                                {
                                                                    s.push_str(&format!(" — {}", t));
                                                                }
                                                                s
                                                            };
                                                            let date_str = ch.discovered_at
                                                                .map(|dt| dt.format("%b %d, %Y").to_string())
                                                                .unwrap_or_default();

                                                            view! {
                                                                <li class="update-chapter-item">
                                                                    <span class="update-chapter-item__label">{label}</span>
                                                                    <span class="update-chapter-item__date">{date_str}</span>
                                                                    {move || {
                                                                        let map = chapters_progress.get();
                                                                        let live = map.get(&chapter_id);
                                                                        let is_active = live.map(|p| {
                                                                            matches!(p.status, LiveChapterStatus::InProgress)
                                                                        }).unwrap_or(false);

                                                                        view! {
                                                                            <button
                                                                                class=move || if is_active {
                                                                                    "download-button download-button--active update-chapter-item__download"
                                                                                } else {
                                                                                    "download-button update-chapter-item__download"
                                                                                }
                                                                                title="Download chapter"
                                                                                disabled=is_active
                                                                                on:click=move |_| {
                                                                                    chapters_progress.update(|m| {
                                                                                        m.insert(chapter_id, crate::types::ChapterProgress {
                                                                                            id: chapter_id,
                                                                                            name: String::new(),
                                                                                            total_pages: 0,
                                                                                            completed_pages: 0,
                                                                                            status: LiveChapterStatus::InProgress,
                                                                                        });
                                                                                    });
                                                                                    leptos::task::spawn_local(async move {
                                                                                        let _ = download_chapter(chapter_id).await;
                                                                                    });
                                                                                }
                                                                            >
                                                                                {if is_active { "⏳" } else { "⬇" }}
                                                                            </button>
                                                                        }
                                                                    }}
                                                                </li>
                                                            }
                                                        }
                                                    />
                                                </ul>
                                            </div>
                                        }
                                    }
                                />
                            </div>

                            <Pagination page set_page has_next=Signal::derive(move || list.has_next_page) />
                        }.into_any()
                    }
                })}
            </Suspense>
        </div>
    }
}