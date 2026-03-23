use crate::{
    pages::components::{
        collapsible_panel::{CollapsiblePanel, CollapsibleVariant},
        source_settings_card::SourceSettingsCard,
        permission_handlers::PermissionGate,
    },
    server_fns::{
        create_category, delete_category, fetch_sources, get_boot_id, get_categories,
        get_settings, rename_category, reorder_categories, update_settings,
    },
    types::{AdvancedSettings, Category, DownloadSettings, ScanSettings, SettingsUpdate, Source},
};
use leptos::{either::Either, prelude::*};

fn add_restart_field(
    set_restart_needed: WriteSignal<bool>,
    pending_fields: RwSignal<std::collections::HashSet<String>>,
    label: &str,
    boot_id: &str,
) {
    let mut fields = pending_fields.get_untracked();
    fields.insert(label.to_string());
    let json = serde_json::to_string(&fields.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    crate::utils::set_local_string("kani_restart_fields", &json);
    crate::utils::set_local_string("kani_restart_boot_id", boot_id);
    crate::utils::set_local_flag("kani_restart_needed", true);
    pending_fields.set(fields);
    set_restart_needed.set(true);
}

fn clear_restart_state(
    set_restart_needed: WriteSignal<bool>,
    pending_fields: RwSignal<std::collections::HashSet<String>>,
) {
    set_restart_needed.set(false);
    pending_fields.set(Default::default());
    crate::utils::set_local_flag("kani_restart_needed", false);
    crate::utils::set_local_string("kani_restart_fields", "[]");
}

#[component]
fn SaveRow(
    pending: ReadSignal<bool>,
    result: ReadSignal<Option<Result<(), String>>>,
    on_save: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="settings-save-row">
            {move || result.get().map(|r| match r {
                Ok(_)  => view! { <span class="settings-save-ok">"✓ Saved"</span> }.into_any(),
                Err(e) => view! { <span class="error">{e}</span> }.into_any(),
            })}
            <button
                class="settings-save-btn"
                disabled=move || pending.get()
                on:click=move |_| on_save.run(())
            >
                {move || if pending.get() { "Saving…" } else { "Save" }}
            </button>
        </div>
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let settings_res   = Resource::new(|| (), |_| get_settings());
    let sources_res    = Resource::new(|| (), |_| fetch_sources());
    let categories_res = Resource::new(|| (), |_| get_categories());
    let boot_id_res    = Resource::new(|| (), |_| get_boot_id());

    let (download_draft, set_download_draft) = signal(Option::<DownloadSettings>::None);
    let (scan_draft,     set_scan_draft)     = signal(Option::<ScanSettings>::None);
    let (advanced_draft, set_advanced_draft) = signal(Option::<AdvancedSettings>::None);

    let (dl_pending,  set_dl_pending)  = signal(false);
    let (dl_result,   set_dl_result)   = signal(Option::<Result<(), String>>::None);
    let (sc_pending,  set_sc_pending)  = signal(false);
    let (sc_result,   set_sc_result)   = signal(Option::<Result<(), String>>::None);
    let (adv_pending, set_adv_pending) = signal(false);
    let (adv_result,  set_adv_result)  = signal(Option::<Result<(), String>>::None);

    let (new_cat_name, set_new_cat_name) = signal(String::new());
    let (cat_error,    set_cat_error)    = signal(Option::<String>::None);
    let (editing_cat,  set_editing_cat)  = signal(Option::<(i64, String)>::None);

    let (restart_needed, set_restart_needed) = signal(false);
    let pending_fields = RwSignal::new(std::collections::HashSet::<String>::new());

    Effect::new(move |_| {
        if let Some(Ok(s)) = settings_res.get() {
            set_download_draft.set(Some(DownloadSettings {
                concurrent_page_downloads:  s.concurrent_page_downloads,
                concurrent_manga_downloads: s.concurrent_manga_downloads,
                chapter_queue_size:         s.chapter_queue_size,
                max_retries:                s.max_retries,
                initial_retry_delay_ms:     s.initial_retry_delay_ms,
            }));
            set_scan_draft.set(Some(ScanSettings {
                auto_scan:             s.auto_scan,
                scan_interval_minutes: s.scan_interval_minutes,
            }));
            set_advanced_draft.set(Some(AdvancedSettings {
                flaresolverr_url:   s.flaresolverr_url,
                library_path:       s.library_path,
                max_wasm_instances: s.max_wasm_instances,
            }));
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(current_id)) = boot_id_res.get()
        && crate::utils::get_local_flag("kani_restart_needed") {
            let stored_id = crate::utils::get_local_string("kani_restart_boot_id");
            if stored_id != current_id {
                clear_restart_state(set_restart_needed, pending_fields);
            } else {
                let json = crate::utils::get_local_string("kani_restart_fields");
                let fields: std::collections::HashSet<String> =
                    serde_json::from_str(&json).unwrap_or_default();
                pending_fields.set(fields);
                set_restart_needed.set(true);
            }
        }
    });

    view! {
        <div class="settings-page">
            <Show when=move || restart_needed.get() fallback=|| ()>
                <div class="restart-banner">
                    <span class="restart-banner__icon">"⚠"</span>
                    <span>
                        "Restart required for: "
                        {move || {
                            let mut fields: Vec<String> =
                                pending_fields.get().into_iter().collect();
                            fields.sort();
                            fields.join(", ")
                        }}
                    </span>
                </div>
            </Show>

            <h1>"Settings"</h1>

            <PermissionGate permission="settings:edit_scan">
            <CollapsiblePanel label="Scan Settings".to_string() open=true variant=CollapsibleVariant::Section>
                {move || match scan_draft.get() {
                    None => view! {
                        <div class="settings-grid">
                            <div class="skeleton-row skeleton-row--xs" style="width: 60%"></div>
                            <div class="skeleton-row skeleton-row--xs" style="width: 40%"></div>
                        </div>
                    }.into_any(),
                    Some(draft) => view! {
                        <div class="settings-grid">
                            <div class="settings-field">
                                <label class="settings-field__label">"Auto-scan for new chapters"</label>
                                <p class="settings-field__hint">
                                    "Automatically check for new chapters on a timer."
                                </p>
                                <label class="toggle-label">
                                    <input
                                        type="checkbox"
                                        checked=draft.auto_scan
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            set_scan_draft.update(|s| {
                                                if let Some(s) = s { s.auto_scan = checked; }
                                            });
                                        }
                                    />
                                    {if draft.auto_scan { " Enabled" } else { " Disabled" }}
                                </label>
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="scan-interval">
                                    "Scan interval (minutes)"
                                </label>
                                <p class="settings-field__hint">"Minimum 5."</p>
                                <input
                                    id="scan-interval"
                                    type="number"
                                    min="5"
                                    max="10080"
                                    prop:value=draft.scan_interval_minutes.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_scan_draft.update(|s| {
                                                if let Some(s) = s { s.scan_interval_minutes = v; }
                                            });
                                        }
                                    }
                                />
                            </div>
                        </div>

                        <SaveRow
                            pending=sc_pending
                            result=sc_result
                            on_save=Callback::new(move |_| {
                                if let Some(draft) = scan_draft.get_untracked() {
                                    set_sc_pending.set(true);
                                    set_sc_result.set(None);
                                    leptos::task::spawn_local(async move {
                                        let res = update_settings(SettingsUpdate::Scan(draft)).await;
                                        set_sc_result.set(Some(res.map_err(|e| e.to_string())));
                                        set_sc_pending.set(false);
                                    });
                                }
                            })
                        />
                    }.into_any()
                }}
            </CollapsiblePanel>
            </PermissionGate>

            <PermissionGate permission="settings:edit_download">
            <CollapsiblePanel label="Download Settings".to_string() open=true variant=CollapsibleVariant::Section>
                {move || match download_draft.get() {
                    None => view! {
                        <div class="settings-grid">
                            {(0..5).map(|_| view! {
                                <div style="display: flex; flex-direction: column; gap: var(--sp-2)">
                                    <div class="skeleton-row skeleton-row--xs" style="width: 70%"></div>
                                    <div class="skeleton-row skeleton-row--xs" style="width: 50%"></div>
                                    <div class="skeleton-row skeleton-row--xs" style="width: 100%"></div>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any(),
                    Some(draft) => view! {
                        <div class="settings-grid">
                            <div class="settings-field">
                                <label class="settings-field__label" for="cpd">
                                    "Concurrent page downloads"
                                </label>
                                <p class="settings-field__hint">
                                    "Pages downloaded in parallel per chapter. Range 1–32."
                                </p>
                                <input id="cpd" type="number" min="1" max="32"
                                    prop:value=draft.concurrent_page_downloads.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_download_draft.update(|s| {
                                                if let Some(s) = s { s.concurrent_page_downloads = v; }
                                            });
                                        }
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="cmd">
                                    "Concurrent manga downloads"
                                </label>
                                <p class="settings-field__hint">
                                    "Chapters processed in parallel. Range 1–16."
                                </p>
                                <input id="cmd" type="number" min="1" max="16"
                                    prop:value=draft.concurrent_manga_downloads.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_download_draft.update(|s| {
                                                if let Some(s) = s { s.concurrent_manga_downloads = v; }
                                            });
                                        }
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="cqs">
                                    "Chapter queue size"
                                </label>
                                <p class="settings-field__hint">
                                    "Max chapters held in the download queue."
                                </p>
                                <input id="cqs" type="number" min="1" max="512"
                                    prop:value=draft.chapter_queue_size.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_download_draft.update(|s| {
                                                if let Some(s) = s { s.chapter_queue_size = v; }
                                            });
                                        }
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="mr">
                                    "Max retries"
                                </label>
                                <p class="settings-field__hint">
                                    "Times to retry a failed page download."
                                </p>
                                <input id="mr" type="number" min="0" max="10"
                                    prop:value=draft.max_retries.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_download_draft.update(|s| {
                                                if let Some(s) = s { s.max_retries = v; }
                                            });
                                        }
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="ird">
                                    "Initial retry delay (ms)"
                                </label>
                                <p class="settings-field__hint">
                                    "Base back-off delay. Doubles on each retry."
                                </p>
                                <input id="ird" type="number" min="50" max="30000"
                                    prop:value=draft.initial_retry_delay_ms.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_download_draft.update(|s| {
                                                if let Some(s) = s { s.initial_retry_delay_ms = v; }
                                            });
                                        }
                                    }
                                />
                            </div>
                        </div>

                        <SaveRow
                            pending=dl_pending
                            result=dl_result
                            on_save=Callback::new(move |_| {
                                if let Some(draft) = download_draft.get_untracked() {
                                    set_dl_pending.set(true);
                                    set_dl_result.set(None);
                                    leptos::task::spawn_local(async move {
                                        let res = update_settings(SettingsUpdate::Download(draft)).await;
                                        set_dl_result.set(Some(res.map_err(|e| e.to_string())));
                                        set_dl_pending.set(false);
                                    });
                                }
                            })
                        />
                    }.into_any()
                }}
            </CollapsiblePanel>
            </PermissionGate>

            <PermissionGate permission="source:browse">
            <CollapsiblePanel label="Source Settings".to_string() open=true variant=CollapsibleVariant::Section>
                <Suspense fallback=move || view! {
                    <div class="skeleton-settings-source-grid">
                        {(0..4).map(|_| view! {
                            <div class="skeleton-settings-source-card"></div>
                        }).collect::<Vec<_>>()}
                    </div>
                }>
                    {move || {
                        let sources = sources_res.get()
                            .and_then(|r| r.ok())
                            .unwrap_or_default();
                        view! {
                            <div class="source-settings-grid">
                                <For
                                    each=move || sources.clone()
                                    key=|s: &Source| s.id
                                    children=move |source| view! { <SourceSettingsCard source=source /> }
                                />
                            </div>
                        }
                    }}
                </Suspense>
            </CollapsiblePanel>
            </PermissionGate>

            <PermissionGate permission="library:manage">
            <CollapsiblePanel label="Category Settings".to_string() open=true variant=CollapsibleVariant::Section>
                <Suspense fallback=move || view! {
                    <div class="skeleton-list">
                        {(0..3).map(|_| view! { <div class="skeleton-row"></div> }).collect::<Vec<_>>()}
                    </div>
                }>
                    {move || {
                        let cats = categories_res.get()
                            .and_then(|r| r.ok())
                            .unwrap_or_default();
                        view! {
                            <ul class="category-manage-list">
                                <For
                                    each=move || cats.clone()
                                    key=|c: &Category| c.id
                                    children=move |cat| {
                                        let cat_id   = cat.id;
                                        let cat_name = cat.name.clone();

                                        let is_editing = Signal::derive(move || {
                                            editing_cat.get().as_ref().map(|(id, _)| *id) == Some(cat_id)
                                        });

                                        view! {
                                            <li class="category-manage-item">
                                                {move || {
                                                    let name_for_input   = cat_name.clone();
                                                    let name_for_display = cat_name.clone();
                                                    let name_for_edit    = cat_name.clone();

                                                    if is_editing.get() {
                                                        Either::Left(view! {
                                                            <>
                                                                <input
                                                                    type="text"
                                                                    prop:value=name_for_input
                                                                    on:input=move |ev| {
                                                                        set_editing_cat.update(|v| {
                                                                            if let Some((id, _)) = v {
                                                                                *v = Some((*id, event_target_value(&ev)));
                                                                            }
                                                                        });
                                                                    }
                                                                />
                                                                <button
                                                                    class="category-manage-item__save"
                                                                    on:click=move |_| {
                                                                        let n = editing_cat.get_untracked()
                                                                            .map(|(_, n)| n)
                                                                            .unwrap_or_default();
                                                                        leptos::task::spawn_local(async move {
                                                                            match rename_category(cat_id, n).await {
                                                                                Ok(_) => {
                                                                                    categories_res.refetch();
                                                                                    set_editing_cat.set(None);
                                                                                }
                                                                                Err(e) => set_cat_error.set(Some(e.to_string())),
                                                                            }
                                                                        });
                                                                    }
                                                                >"Save"</button>
                                                                <button
                                                                    class="category-manage-item__cancel"
                                                                    on:click=move |_| set_editing_cat.set(None)
                                                                >"Cancel"</button>
                                                            </>
                                                        })
                                                    } else {
                                                        Either::Right(view! {
                                                            <>
                                                                <span class="category-manage-item__name">
                                                                    {name_for_display}
                                                                </span>
                                                                <div class="category-manage-item__actions">
                                                                    <button
                                                                        class="category-manage-item__edit"
                                                                        on:click=move |_| {
                                                                            set_editing_cat.set(Some((cat_id, name_for_edit.clone())));
                                                                        }
                                                                    >"Edit"</button>
                                                                    <button
                                                                        class="category-manage-item__delete"
                                                                        on:click=move |_| {
                                                                            leptos::task::spawn_local(async move {
                                                                                match delete_category(cat_id).await {
                                                                                    Ok(_) => {
                                                                                        let current_cats = categories_res
                                                                                            .get_untracked()
                                                                                            .and_then(|r| r.ok())
                                                                                            .unwrap_or_default();
                                                                                        let remaining: Vec<i64> = current_cats
                                                                                            .iter()
                                                                                            .filter(|c| c.id != cat_id)
                                                                                            .map(|c| c.id)
                                                                                            .collect();
                                                                                        let _ = reorder_categories(remaining).await;
                                                                                        categories_res.refetch();
                                                                                    }
                                                                                    Err(e) => set_cat_error.set(Some(e.to_string())),
                                                                                }
                                                                            });
                                                                        }
                                                                    >"Delete"</button>
                                                                </div>
                                                            </>
                                                        })
                                                    }
                                                }}
                                            </li>
                                        }
                                    }
                                />
                            </ul>

                            {move || cat_error.get().map(|e| view! { <p class="error">{e}</p> })}

                            <div class="category-add-row">
                                <input
                                    type="text"
                                    placeholder="New category name…"
                                    prop:value=move || new_cat_name.get()
                                    on:input=move |ev| set_new_cat_name.set(event_target_value(&ev))
                                />
                                <button
                                    class="category-add-btn"
                                    on:click=move |_| {
                                        let name = new_cat_name.get_untracked();
                                        let next_order = categories_res.get()
                                            .and_then(|r| r.ok())
                                            .map(|v| v.len() as i64)
                                            .unwrap_or(0);
                                        leptos::task::spawn_local(async move {
                                            match create_category(name, next_order).await {
                                                Ok(_) => {
                                                    set_new_cat_name.set(String::new());
                                                    set_cat_error.set(None);
                                                    categories_res.refetch();
                                                }
                                                Err(e) => set_cat_error.set(Some(e.to_string())),
                                            }
                                        });
                                    }
                                >"+ Add Category"</button>
                            </div>
                        }
                    }}
                </Suspense>
            </CollapsiblePanel>
            </PermissionGate>

            <PermissionGate permission="settings:edit_advanced">
            <CollapsiblePanel label="Advanced Settings".to_string() open=false variant=CollapsibleVariant::Section>
                {move || match advanced_draft.get() {
                    None => view! {
                        <div class="settings-grid">
                            {(0..3).map(|_| view! {
                                <div style="display: flex; flex-direction: column; gap: var(--sp-2)">
                                    <div class="skeleton-row skeleton-row--xs" style="width: 50%"></div>
                                    <div class="skeleton-row skeleton-row--xs" style="width: 100%"></div>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any(),
                    Some(draft) => view! {
                        <div class="settings-grid">
                            <div class="settings-field">
                                <label class="settings-field__label" for="fsr-url">
                                    "FlareSolverr URL"
                                </label>
                                <p class="settings-field__hint">
                                    "Used to bypass Cloudflare protection. Leave empty to disable."
                                </p>
                                <input
                                    id="fsr-url"
                                    type="text"
                                    prop:value=draft.flaresolverr_url.clone()
                                    on:change=move |ev| {
                                        set_advanced_draft.update(|s| {
                                            if let Some(s) = s { s.flaresolverr_url = event_target_value(&ev); }
                                        });
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label">"Library path"</label>
                                <div class="settings-field settings-field--needs-restart">
                                    <p class="settings-field__hint settings-field__hint--warn">
                                        "Changing this requires a server restart and will not move existing files."
                                    </p>
                                </div>
                                <input
                                    type="text"
                                    prop:value=draft.library_path.clone()
                                    on:change=move |ev| {
                                        let id = boot_id_res.get().and_then(|r| r.ok()).unwrap_or_default();
                                        add_restart_field(set_restart_needed, pending_fields, "Library path", &id);
                                        set_advanced_draft.update(|s| {
                                            if let Some(s) = s { s.library_path = event_target_value(&ev); }
                                        });
                                    }
                                />
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label" for="mwi">
                                    "Max WASM instances"
                                </label>
                                <div class="settings-field settings-field--needs-restart">
                                    <p class="settings-field__hint settings-field__hint--warn">
                                        "Requires server restart."
                                    </p>
                                </div>
                                <input id="mwi" type="number" min="1" max="10000"
                                    prop:value=draft.max_wasm_instances.to_string()
                                    on:change=move |ev| {
                                        let id = boot_id_res.get().and_then(|r| r.ok()).unwrap_or_default();
                                        add_restart_field(set_restart_needed, pending_fields, "Max WASM instances", &id);
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_advanced_draft.update(|s| {
                                                if let Some(s) = s { s.max_wasm_instances = v; }
                                            });
                                        }
                                    }
                                />
                            </div>
                        </div>

                        <SaveRow
                            pending=adv_pending
                            result=adv_result
                            on_save=Callback::new(move |_| {
                                if let Some(draft) = advanced_draft.get_untracked() {
                                    set_adv_pending.set(true);
                                    set_adv_result.set(None);
                                    leptos::task::spawn_local(async move {
                                        let res = update_settings(SettingsUpdate::Advanced(draft)).await;
                                        set_adv_result.set(Some(res.map_err(|e| e.to_string())));
                                        set_adv_pending.set(false);
                                    });
                                }
                            })
                        />
                    }.into_any()
                }}
            </CollapsiblePanel>
            </PermissionGate>
        </div>
    }
}