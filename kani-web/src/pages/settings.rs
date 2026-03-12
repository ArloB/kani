use crate::{
    server_fns::{
        create_category, delete_category, fetch_sources,
        get_categories, get_settings, rename_category, reorder_categories,
        update_settings,
    },
    types::{AppSettings, Category, Source},
};
use leptos::{either::Either, prelude::*};

#[component]
pub fn Settings() -> impl IntoView {
    let (scan_open,     set_scan_open)     = signal(true);
    let (download_open, set_download_open) = signal(true);
    let (sources_open,  set_sources_open)  = signal(true);
    let (cats_open,     set_cats_open)     = signal(true);
    let (advanced_open, set_advanced_open) = signal(false);

    let settings_res  = Resource::new(|| (), |_| get_settings());
    let sources_res   = Resource::new(|| (), |_| fetch_sources());
    let categories_res = Resource::new(|| (), |_| get_categories());

    let (settings_draft, set_settings_draft) = signal(Option::<AppSettings>::None);
    let (save_pending,   set_save_pending)   = signal(false);
    let (save_msg,       set_save_msg)       = signal(Option::<Result<(), String>>::None);

    Effect::new(move |_| {
        if let Some(Ok(s)) = settings_res.get() {
            set_settings_draft.set(Some(s));
        }
    });

    let (new_cat_name,  set_new_cat_name)  = signal(String::new());
    let (cat_error,     set_cat_error)     = signal(Option::<String>::None);
    let (editing_cat,   set_editing_cat)   = signal(Option::<(i64, String)>::None);

    view! {
        <div class="settings-page">
            <h1>"Settings"</h1>
            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_scan_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">"Scan Settings"</span>
                    <span class="settings-section__chevron">
                        {move || if scan_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>

                <Show when=move || scan_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        {move || settings_draft.get().map(|draft| view! {
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
                                            set_settings_draft.update(|s| {
                                                if let Some(s) = s { s.auto_scan = checked; }
                                            });
                                        }
                                    />
                                    {if draft.auto_scan { " Enabled" } else { " Disabled" }}
                                </label>
                            </div>

                            <div class="settings-field">
                                <label class="settings-field__label"
                                    for="scan-interval"
                                >"Scan interval (minutes)"</label>
                                <p class="settings-field__hint">
                                    "Minimum 5. Changes take effect after saving."
                                </p>
                                <input
                                    id="scan-interval"
                                    type="number"
                                    min="5"
                                    max="10080"
                                    prop:value=draft.scan_interval_minutes.to_string()
                                    on:change=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                            set_settings_draft.update(|s| {
                                                if let Some(s) = s { s.scan_interval_minutes = v; }
                                            });
                                        }
                                    }
                                />
                            </div>
                        })}
                    </div>
                </Show>
            </section>

            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_download_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">"Download Settings"</span>
                    <span class="settings-section__chevron">
                        {move || if download_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>

                <Show when=move || download_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        {move || settings_draft.get().map(|draft| {
                            view! {
                                <div class="settings-grid">
                                    <div class="settings-field">
                                        <label class="settings-field__label" for="cpd">
                                            "Concurrent page downloads"
                                        </label>
                                        <p class="settings-field__hint">
                                            "Pages downloaded in parallel per chapter. Range 1-32."
                                        </p>
                                        <input id="cpd" type="number" min="1" max="32"
                                            prop:value=draft.concurrent_page_downloads.to_string()
                                            on:change=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                                    set_settings_draft.update(|s| {
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
                                            "Chapters processed in parallel. Range 1-16."
                                        </p>
                                        <input id="cmd" type="number" min="1" max="16"
                                            prop:value=draft.concurrent_manga_downloads.to_string()
                                            on:change=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                                    set_settings_draft.update(|s| {
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
                                                    set_settings_draft.update(|s| {
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
                                                    set_settings_draft.update(|s| {
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
                                                    set_settings_draft.update(|s| {
                                                        if let Some(s) = s { s.initial_retry_delay_ms = v; }
                                                    });
                                                }
                                            }
                                        />
                                    </div>
                                </div>
                            }
                        })}
                    </div>
                </Show>
            </section>

            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_sources_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">"Source Settings"</span>
                    <span class="settings-section__chevron">
                        {move || if sources_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>

                <Show when=move || sources_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        <Suspense fallback=move || view! { <p class="spinner">"Loading sources…"</p> }>
                            {move || {
                                let sources = sources_res.get()
                                    .and_then(|r| r.ok())
                                    .unwrap_or_default();

                                view! {
                                    <div class="source-settings-grid">
                                        <For
                                            each=move || sources.clone()
                                            key=|s: &Source| s.id
                                            children=move |source| {
                                                view! { <SourceSettingsCard source=source /> }
                                            }
                                        />
                                    </div>
                                }
                            }}
                        </Suspense>
                    </div>
                </Show>
            </section>

            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_cats_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">"Categories"</span>
                    <span class="settings-section__chevron">
                        {move || if cats_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>

                <Show when=move || cats_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        <Suspense fallback=move || view! { <p class="spinner">"Loading…"</p> }>
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
                                              let cat_id = cat.id;
                                              let cat_name = cat.name.clone();

                                              let is_editing = Signal::derive(move || {
                                                  editing_cat.get().as_ref().map(|(id, _)| *id) == Some(cat_id)
                                              });

                                              view! {
                                                  <li class="category-manage-item">
                                                      {move || {
                                                          let name_for_input = cat_name.clone();
                                                          let name_for_display = cat_name.clone();
                                                          let name_for_edit = cat_name.clone();

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
                                                                      <span class="category-manage-item__name">{name_for_display}</span>
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

                                    {move || cat_error.get().map(|e| view! {
                                        <p class="error">{e}</p>
                                    })}

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
                    </div>
                </Show>
            </section>

            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_advanced_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">"Advanced"</span>
                    <span class="settings-section__chevron">
                        {move || if advanced_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>

                <Show when=move || advanced_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        {move || settings_draft.get().map(|draft| view! {
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
                                            set_settings_draft.update(|s| {
                                                if let Some(s) = s { s.flaresolverr_url = event_target_value(&ev); }
                                            });
                                        }
                                    />
                                </div>

                                <div class="settings-field">
                                    <label class="settings-field__label">"Library path"</label>
                                    <p class="settings-field__hint settings-field__hint--warn">
                                        "Changing this requires a server restart and will not move existing files."
                                    </p>
                                    <input
                                        type="text"
                                        prop:value=draft.library_path.clone()
                                        on:change=move |ev| {
                                            set_settings_draft.update(|s| {
                                                if let Some(s) = s { s.library_path = event_target_value(&ev); }
                                            });
                                        }
                                    />
                                </div>

                                <div class="settings-field">
                                    <label class="settings-field__label" for="mwi">
                                        "Max WASM instances"
                                    </label>
                                    <p class="settings-field__hint settings-field__hint--warn">
                                        "Requires server restart."
                                    </p>
                                    <input id="mwi" type="number" min="1" max="10000"
                                        prop:value=draft.max_wasm_instances.to_string()
                                        on:change=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                                set_settings_draft.update(|s| {
                                                    if let Some(s) = s { s.max_wasm_instances = v; }
                                                });
                                            }
                                        }
                                    />
                                </div>
                            </div>
                        })}
                    </div>
                </Show>
            </section>

            <div class="settings-save-row">
                {move || save_msg.get().map(|r| match r {
                    Ok(_)  => view! { <span class="settings-save-ok">"✓ Saved"</span> }.into_any(),
                    Err(e) => view! { <span class="error">{e}</span> }.into_any(),
                })}
                <button
                    class="settings-save-btn"
                    disabled=move || save_pending.get() || settings_draft.get().is_none()
                    on:click=move |_| {
                        if let Some(draft) = settings_draft.get_untracked() {
                            set_save_pending.set(true);
                            set_save_msg.set(None);
                            leptos::task::spawn_local(async move {
                                let result = update_settings(draft).await;
                                set_save_msg.set(Some(result.map_err(|e| e.to_string())));
                                set_save_pending.set(false);
                            });
                        }
                    }
                >
                    {move || if save_pending.get() { "Saving…" } else { "Save Settings" }}
                </button>
            </div>
        </div>
    }
}

#[component]
fn SourceSettingsCard(source: Source) -> impl IntoView {
    view! {
        <div class="source-settings-card">
            <div class="source-settings-card__header">
                <div class="source-settings-card__meta">
                    <span class="source-settings-card__name">{source.name.clone()}</span>
                    <span class="source-settings-card__version">{source.version.clone()}</span>
                </div>
            </div>
            <p class="source-settings-card__placeholder">
                "Source-specific preferences will appear here."
            </p>
        </div>
    }
}