use leptos::prelude::*;
use crate::types::LiveChapterStatus;

#[component]
pub fn DownloadProgress() -> impl IntoView {
    let chapters = expect_context::<RwSignal<std::collections::HashMap<i64, crate::types::ChapterProgress>>>();

    let dismiss = move |id: i64| {
        chapters.update(|map| {
            map.remove(&id);
        });
    };

    let has_visible = move || chapters.with(|m| m.values().any(|c| !matches!(c.status, LiveChapterStatus::Deleted | LiveChapterStatus::CompletedHidden)));

    let entries = move || {
        let mut entries: Vec<crate::types::ChapterProgress> = chapters.with(|map| {
            map.values()
                .filter(|c| !matches!(c.status, LiveChapterStatus::Deleted | LiveChapterStatus::CompletedHidden))
                .cloned()
                .collect()
        });
        entries.sort_by_key(|chapter| match chapter.status {
            LiveChapterStatus::InProgress => 0,
            _ => 1,
        });
        entries
    };

    view! {
        <Show
            when=has_visible
            fallback=|| view! { <div></div> }
        >
            <div class="download-progress-overlay">
                <div class="download-progress-header">
                    <span class="download-progress-title">"Downloads"</span>
                </div>
                <div class="download-progress-list">
                    <For
                        each=entries
                        key=|c| c.id
                        children=move |chapter| {
                            let id = chapter.id;
                            let chapter_sig = Signal::derive(move || {
                                chapters.with(|m| m.get(&id).cloned().unwrap_or_else(|| chapter.clone()))
                            });

                            let is_done = move || !matches!(chapter_sig.get().status, LiveChapterStatus::InProgress);
                            
                            view! {
                                <div class="download-item">
                                    <div class="download-item-header">
                                        <span class="download-item-name">{move || chapter_sig.get().name}</span>
                                        <span class="download-item-status">
                                            {move || {
                                                let c = chapter_sig.get();
                                                match c.status {
                                                    LiveChapterStatus::InProgress => format!("{}/{} pages", c.completed_pages, c.total_pages),
                                                    LiveChapterStatus::Completed  => "Complete".to_string(),
                                                    LiveChapterStatus::Failed(e)  => format!("Failed: {}", e),
                                                    LiveChapterStatus::Cancelled  => "Cancelled".to_string(),
                                                    _ => "".to_string(),
                                                }
                                            }}
                                        </span>
                                        <Show when=is_done fallback=|| ()>
                                            <button
                                                class="download-item-dismiss"
                                                on:click=move |_| dismiss(id)
                                            >
                                                "✕"
                                            </button>
                                        </Show>
                                    </div>
                                    <div class="progress-track">
                                        <div
                                            class=move || {
                                                match chapter_sig.get().status {
                                                    LiveChapterStatus::InProgress=>"progress-bar progress-bar--active",
                                                    LiveChapterStatus::Completed=>"progress-bar progress-bar--done",
                                                    LiveChapterStatus::Failed(_)=>"progress-bar progress-bar--failed",
                                                    LiveChapterStatus::Cancelled=>"progress-bar progress-bar--failed",
                                                    _ => "",
                                                }
                                            }
                                            style=move || format!("width: {:.1}%", chapter_sig.get().completion_pct())
                                        ></div>
                                    </div>
                                </div>
                            }
                        }
                    />
                </div>
            </div>
        </Show>
    }
}
