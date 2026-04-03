use leptos::{prelude::*, web_sys};
use std::collections::HashMap;
use crate::{
    server_fns::{
        delete_source, fetch_wasm_url, get_active_source_ids,
        get_source_preference_schema, get_source_preferences,
        toggle_source_enabled, toggle_source_favourite,
    },
    types::{PreferenceDescriptor, Source},
    pages::components::preference_row::PreferenceRow,
    pages::components::permission_handlers::PermissionGate,
    pages::components::toggle::Toggle,
};

#[component]
pub fn SourceSettingsCard(
    source: Source,
    on_deleted: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    let sid = source.id;
    let is_unsafe = source.unrestricted_http;
    let on_deleted = StoredValue::new(on_deleted);

    let (enabled, set_enabled) = signal(source.enabled);
    let (starred, set_starred) = signal(source.favourited);
    let (confirming, set_confirming) = signal(false);
    let (confirming_delete, set_confirming_delete) = signal(false);
    let (modal_open, set_modal_open) = signal(false);
    let (install_open, set_install_open) = signal(false);
    let (wasm_url, set_wasm_url) = signal(String::new());
    let (wasm_fetching, set_wasm_fetching) = signal(false);
    let (wasm_error, set_wasm_error) = signal(Option::<String>::None);
    let (wasm_success, set_wasm_success) = signal(false);

    let active_ids_res = Resource::new(|| (), |_| get_active_source_ids());
    let is_active = move || {
        active_ids_res.get()
            .and_then(|r| r.ok())
            .map(|ids| ids.contains(&sid))
            .unwrap_or(false)
    };

    let schema_res = Resource::new(move || sid, get_source_preference_schema);
    let values_res = Resource::new(move || sid, get_source_preferences);

    let live_values: RwSignal<HashMap<String, String>> = RwSignal::new(HashMap::new());

    Effect::new(move |_| {
        if let Some(Ok(vals)) = values_res.get() {
            live_values.set(vals.into_iter().collect());
        }
    });


    let close_on_escape = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" {
            set_modal_open.set(false);
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
                        <Suspense fallback=|| ()>
                            {move || {
                                let active = is_active();
                                view! {
                                    <span
                                        class="source-status-badge"
                                        class:source-status-badge--active=active
                                        class:source-status-badge--inactive=!active
                                    >
                                        {if active { "Active" } else { "No WASM" }}
                                    </span>
                                }
                            }}
                        </Suspense>
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
                    <Suspense fallback=|| ()>
                        {move || {
                            let has_prefs = schema_res.get()
                                .and_then(|r| r.ok())
                                .map(|s| !s.is_empty())
                                .unwrap_or(false);
                            if has_prefs {
                                view! {
                                    <PermissionGate permission="source:configure">
                                        <button
                                            class="source-settings-card__configure-btn"
                                            title="Configure source"
                                            on:click=move |_| set_modal_open.set(true)
                                        >
                                            "⚙"
                                        </button>
                                    </PermissionGate>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                    </Suspense>
                    <PermissionGate permission="source:browse">
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
                    </PermissionGate>
                    <PermissionGate permission="source:install">
                        <button
                            class="source-settings-card__install-btn"
                            title="Install WASM"
                            on:click=move |_| {
                                set_install_open.update(|v| *v = !*v);
                                set_wasm_error.set(None);
                                set_wasm_success.set(false);
                            }
                        >
                            "↑"
                        </button>
                    </PermissionGate>
                    <PermissionGate permission="source:delete">
                        <button
                            class="source-settings-card__delete-btn"
                            title="Delete source"
                            on:click=move |_| set_confirming_delete.set(true)
                        >
                            "✕"
                        </button>
                    </PermissionGate>
                    <PermissionGate permission="source:toggle_enabled">
                        <Toggle
                            checked=enabled.into()
                            on_change=move |val| {
                                if val && is_unsafe && !enabled.get() {
                                    set_confirming.set(true);
                                } else {
                                    set_enabled.set(val);
                                    leptos::task::spawn_local(async move {
                                        let _ = toggle_source_enabled(sid, val).await;
                                    });
                                }
                            }
                        >
                            {move || if enabled.get() { " On" } else { " Off" }}
                        </Toggle>
                    </PermissionGate>
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

            <Show when=move || install_open.get()>
                <div class="source-install-panel">
                    <div class="source-install-panel__row">
                        <input
                            type="url"
                            class="source-install-panel__url"
                            placeholder="https://example.com/extension.wasm"
                            prop:value=wasm_url
                            on:input=move |ev| {
                                set_wasm_url.set(event_target_value(&ev));
                                set_wasm_error.set(None);
                                set_wasm_success.set(false);
                            }
                        />
                        <button
                            class="source-install-panel__fetch-btn"
                            disabled=move || wasm_fetching.get() || wasm_url.get().trim().is_empty()
                            on:click=move |_| {
                                let url = wasm_url.get().trim().to_string();
                                if url.is_empty() { return; }
                                set_wasm_fetching.set(true);
                                set_wasm_error.set(None);
                                set_wasm_success.set(false);
                                leptos::task::spawn_local(async move {
                                    match fetch_wasm_url(sid, url).await {
                                        Ok(_) => {
                                            set_wasm_success.set(true);
                                            set_wasm_url.set(String::new());
                                            active_ids_res.refetch();
                                        }
                                        Err(e) => set_wasm_error.set(Some(e.to_string())),
                                    }
                                    set_wasm_fetching.set(false);
                                });
                            }
                        >
                            {move || if wasm_fetching.get() { "Fetching…" } else { "Fetch" }}
                        </button>
                    </div>
                    {move || wasm_error.get().map(|e| view! {
                        <p class="source-install-panel__error">{e}</p>
                    })}
                    {move || wasm_success.get().then(|| view! {
                        <p class="source-install-panel__success">"WASM installed successfully."</p>
                    })}
                </div>
            </Show>

            <Show when=move || confirming_delete.get()>
                <div class="source-delete-confirm">
                    <p class="source-delete-confirm__text">
                        "Delete this source? This will remove all associated manga and chapters."
                    </p>
                    <div class="source-delete-confirm__actions">
                        <button
                            class="source-delete-confirm__cancel"
                            on:click=move |_| set_confirming_delete.set(false)
                        >
                            "Cancel"
                        </button>
                        <button
                            class="source-delete-confirm__accept"
                            on:click=move |_| {
                                set_confirming_delete.set(false);
                                leptos::task::spawn_local(async move {
                                    let _ = delete_source(sid).await;
                                    on_deleted.with_value(|f| f());
                                });
                            }
                        >
                            "Delete"
                        </button>
                    </div>
                </div>
            </Show>
        </div>

        <Show when=move || modal_open.get()>
            <div
                class="modal-overlay"
                on:click=move |ev| {
                    if ev.target() == ev.current_target() {
                        set_modal_open.set(false);
                    }
                }
                on:keydown=close_on_escape
            >
                <div class="modal modal--wide" role="dialog" aria-modal="true">
                    <div class="modal-header">
                        <h2>
                            {source.name.clone()}
                            " — Settings"
                        </h2>
                        <button
                            class="modal-close"
                            aria-label="Close"
                            on:click=move |_| set_modal_open.set(false)
                        >
                            "×"
                        </button>
                    </div>

                    <div class="modal-body">
                        <Show when=move || is_unsafe>
                            <div class="modal-notice modal-notice--warn">
                                "⚠ This extension has unrestricted network access and \
                                 can contact any server."
                            </div>
                        </Show>

                        <Suspense fallback=|| view! {
                            <p class="source-settings-card__loading">"Loading preferences…"</p>
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

                                let mut groups: Vec<(Option<String>, Vec<PreferenceDescriptor>)> =
                                    Vec::new();
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
                                            key=|(group_name, _)| {
                                                group_name.clone().unwrap_or_default()
                                            }
                                            children=move |(group_name, descriptors)| {
                                                view! {
                                                    <div class="source-pref-group">
                                                        {group_name.map(|name| view! {
                                                            <p class="source-pref-group__label">
                                                                {name}
                                                            </p>
                                                        })}
                                                        <For
                                                            each=move || descriptors.clone()
                                                            key=|d| d.key.clone()
                                                            children=move |desc| {
                                                                let current = live_values
                                                                    .get()
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
                </div>
            </div>
        </Show>
    }
}