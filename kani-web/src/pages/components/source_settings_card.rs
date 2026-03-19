use leptos::{prelude::*, web_sys};
use std::collections::HashMap;
use crate::{
    server_fns::{
        get_source_preference_schema, get_source_preferences, toggle_source_enabled, toggle_source_favourite
    },
    types::{PreferenceDescriptor, Source},
    pages::components::preference_row::PreferenceRow
};

#[component]
pub fn SourceSettingsCard(source: Source) -> impl IntoView {
    let sid = source.id;
    let is_unsafe = source.unrestricted_http;

    let (enabled, set_enabled) = signal(source.enabled);
    let (starred, set_starred) = signal(source.favourited);
    let (confirming, set_confirming) = signal(false);

    let schema_res = Resource::new(move || sid, get_source_preference_schema);
    let values_res = Resource::new(move || sid, get_source_preferences);

    let live_values: RwSignal<HashMap<String, String>> = RwSignal::new(HashMap::new());

    Effect::new(move |_| {
        if let Some(Ok(vals)) = values_res.get() {
            live_values.set(vals.into_iter().collect());
        }
    });

    let handle_enable_toggle = move |ev: web_sys::Event| {
        let val = event_target_checked(&ev);
        if val && is_unsafe && !enabled.get() {
            set_confirming.set(true);
        } else {
            set_enabled.set(val);
            leptos::task::spawn_local(async move {
                let _ = toggle_source_enabled(sid, val).await;
            });
        }
    };

    view! {
        <div
            class="source-settings-card"
            class:source-settings-card--disabled=move || !enabled.get()
            class:source-settings-card--unsafe=is_unsafe
        >
            <div class="source-settings-card__header">
                <div class="source-settings-card__meta">
                    <div class="source-settings-card__name-row">
                        <span class="source-settings-card__name">{source.name.clone()}</span>
                        <Show when=move || is_unsafe>
                            <span
                                class="source-unsafe-badge"
                                title="This extension can contact any server on the internet, \
                                       not just its declared source. Only enable if you trust it."
                            >
                                "⚠ Unrestricted"
                            </span>
                        </Show>
                    </div>
                    <span class="source-settings-card__version">{source.version.clone()}</span>
                </div>
                <div class="source-settings-card__actions">
                    <label class="star-checkbox" title="Favourite">
                        <input
                            type="checkbox"
                            checked=move || starred.get()
                            on:change=move |ev| {
                                let val = event_target_checked(&ev);
                                set_starred.set(val);
                                leptos::task::spawn_local(async move {
                                    let _ = toggle_source_favourite(sid, val).await;
                                });
                            }
                        />
                        <span class="star-checkbox__icon">
                            {move || if starred.get() { "★" } else { "☆" }}
                        </span>
                    </label>
                    <label class="toggle-label" title="Enable source">
                        <input
                            type="checkbox"
                            checked=move || enabled.get()
                            on:change=handle_enable_toggle
                        />
                        {move || if enabled.get() { " On" } else { " Off" }}
                    </label>
                </div>
            </div>

            <Show when=move || confirming.get()>
                <div class="source-unsafe-confirm">
                    <p class="source-unsafe-confirm__text">
                        "⚠ This extension bypasses network restrictions and can \
                         contact any server. Only enable extensions from sources you trust."
                    </p>
                    <div class="source-unsafe-confirm__actions">
                        <button
                            class="source-unsafe-confirm__cancel"
                            on:click=move |_| set_confirming.set(false)
                        >
                            "Cancel"
                        </button>
                        <button
                            class="source-unsafe-confirm__accept"
                            on:click=move |_| {
                                set_confirming.set(false);
                                set_enabled.set(true);
                                leptos::task::spawn_local(async move {
                                    let _ = toggle_source_enabled(sid, true).await;
                                });
                            }
                        >
                            "Enable anyway"
                        </button>
                    </div>
                </div>
            </Show>

            <Suspense fallback=|| view! {
                <p class="source-settings-card__loading">"Loading…"</p>
            }>
                {move || {
                    let schema = schema_res.get()
                        .and_then(|r| r.ok())
                        .unwrap_or_default();

                    if schema.is_empty() {
                        return view! {
                            <p class="source-settings-card__placeholder">
                                "No configurable options."
                            </p>
                        }.into_any();
                    }

                    let mut groups: Vec<(Option<String>, Vec<PreferenceDescriptor>)> = Vec::new();
                    for desc in schema {
                        let g = desc.group.clone();
                        if let Some(entry) = groups.iter_mut().find(|(k, _)| k == &g) {
                            entry.1.push(desc);
                        } else {
                            groups.push((g, vec![desc]));
                        }
                    }

                    view! {
                        <div class="source-pref-body">
                            <For
                                each=move || groups.clone()
                                key=|(group_name, _)| group_name.clone().unwrap_or_default()
                                children=move |(group_name, descriptors)| {
                                    view! {
                                        <div class="source-pref-group">
                                            {group_name.map(|name| view! {
                                                <p class="source-pref-group__label">{name}</p>
                                            })}
                                            <For
                                                each=move || descriptors.clone()
                                                key=|d| d.key.clone()
                                                children=move |desc| {
                                                    let current = live_values.get()
                                                        .get(&desc.key)
                                                        .cloned()
                                                        .unwrap_or_default();
                                                    view! {
                                                        <PreferenceRow
                                                            source_id=sid
                                                            descriptor=desc
                                                            current_value=current
                                                            live_values=live_values
                                                        />
                                                    }
                                                }
                                            />
                                        </div>
                                    }
                                }
                            />
                        </div>
                    }.into_any()
                }}
            </Suspense>
        </div>
    }
}