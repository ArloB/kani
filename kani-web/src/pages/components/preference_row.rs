use leptos::prelude::*;
use std::collections::HashMap;
use crate::types::{PreferenceDescriptor, PreferenceKind};
use crate::server_fns::{
    append_preference_list_item, get_source_preferences, remove_preference_list_item, set_source_preference, toggle_preference_select_item
};

fn pref_is_truthy(value: &str) -> bool {
    match value.trim() {
        "" | "null" | "false" | "0" => false,
        v => {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(v) {
                return !arr.is_empty();
            }
            true
        }
    }
}

#[component]
pub fn PreferenceRow(
    source_id: i64,
    descriptor: PreferenceDescriptor,
    current_value: String,
    live_values: RwSignal<HashMap<String, String>>,
) -> impl IntoView {
    let requires = descriptor.requires_key.clone();
    let title = descriptor.title.clone();
    let description = descriptor.description.clone();
    let key_for_label = descriptor.key.clone();

    let visible = Signal::derive(move || {
        match &requires {
            None => true,
            Some(dep_key) => {
                let dep_key = dep_key.clone();
                live_values.get()
                    .get(&dep_key)
                    .map(|v| pref_is_truthy(v))
                    .unwrap_or(false)
            }
        }
    });

    let key = descriptor.key.clone();

    let is_label = matches!(descriptor.kind, PreferenceKind::Label { .. });

    let widget = match descriptor.kind {
        PreferenceKind::Label { text } => view! {
            <p class="pref-row__label-text">{text}</p>
        }.into_any(),

        PreferenceKind::TextInput { placeholder, is_secret, .. } => {
            let display = serde_json::from_str::<String>(&current_value)
                .unwrap_or(current_value.clone());
            let (draft, set_draft) = signal(display);
            let k = key.clone();
            view! {
                <div class="pref-row__text-row">
                    <input
                        class="pref-row__text-input"
                        type=if is_secret { "password" } else { "text" }
                        placeholder=placeholder.unwrap_or_default()
                        prop:value=move || draft.get()
                        on:input=move |ev| set_draft.set(event_target_value(&ev))
                    />
                    <button class="pref-row__save-btn" on:click={
                        let k2 = k.clone();
                        move |_| {
                            let raw = serde_json::to_string(&draft.get_untracked()).unwrap();
                            let k3 = k2.clone();
                            let lv = live_values;
                            leptos::task::spawn_local(async move {
                                let _ = set_source_preference(source_id, k3.clone(), raw.clone()).await;
                                lv.update(|m| { m.insert(k3, raw); });
                            });
                        }
                    }>"Save"</button>
                </div>
            }.into_any()
        },

        PreferenceKind::Checkbox { .. } => {
            let checked = current_value == "true";
            let (val, set_val) = signal(checked);
            let k = key.clone();
            view! {
                <label class="pref-row__checkbox-label">
                    <input
                        class="pref-row__checkbox"
                        type="checkbox"
                        checked=move || val.get()
                        on:change=move |ev| {
                            let v = event_target_checked(&ev);
                            set_val.set(v);
                            let encoded = if v { "true".to_string() } else { "false".to_string() };
                            let k2 = k.clone();
                            let lv = live_values;
                            leptos::task::spawn_local(async move {
                                let _ = set_source_preference(
                                    source_id, k2.clone(), encoded.clone()
                                ).await;
                                lv.update(|m| { m.insert(k2, encoded); });
                            });
                        }
                    />
                    <span class="pref-row__checkbox-track"></span>
                </label>
            }.into_any()
        },

        PreferenceKind::Number { min, max, step, .. } => {
            let display = serde_json::from_str::<f64>(&current_value)
                .unwrap_or(0.0)
                .to_string();
            let (draft, set_draft) = signal(display);
            let k = key.clone();
            view! {
                <div class="pref-row__number-row">
                    <input
                        class="pref-row__number-input"
                        type="number"
                        min=min.map(|v| v.to_string()).unwrap_or_default()
                        max=max.map(|v| v.to_string()).unwrap_or_default()
                        step=step.map(|v| v.to_string()).unwrap_or("any".to_string())
                        prop:value=move || draft.get()
                        on:change=move |ev| {
                            let raw_str = event_target_value(&ev);
                            set_draft.set(raw_str.clone());
                            // Validate and store as a JSON number
                            if let Ok(num) = raw_str.parse::<f64>() {
                                let encoded = num.to_string(); // bare JSON number
                                let k2 = k.clone();
                                let lv = live_values;
                                leptos::task::spawn_local(async move {
                                    let _ = set_source_preference(
                                        source_id, k2.clone(), encoded.clone()
                                    ).await;
                                    lv.update(|m| { m.insert(k2, encoded); });
                                });
                            }
                        }
                    />
                </div>
            }.into_any()
        },

        PreferenceKind::Select { options, .. } => {
            let cur = serde_json::from_str::<String>(&current_value)
                .unwrap_or(current_value.clone());
            let k = key.clone();
            view! {
                <select class="pref-row__select"
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        let encoded = serde_json::to_string(&v).unwrap();
                        let k2 = k.clone();
                        let lv = live_values;
                        leptos::task::spawn_local(async move {
                            let _ = set_source_preference(
                                source_id, k2.clone(), encoded.clone()
                            ).await;
                            lv.update(|m| { m.insert(k2, encoded); });
                        });
                    }
                >
                    <For each=move || options.clone() key=|o| o.value.clone()
                        children=move |opt| {
                            let selected = opt.value == cur;
                            view! {
                                <option value=opt.value.clone() selected=selected>
                                    {opt.label.clone()}
                                </option>
                            }
                        }
                    />
                </select>
            }.into_any()
        },

        PreferenceKind::MultiSelect { options, .. } => {
            let selected_values: Vec<String> =
                serde_json::from_str(&current_value).unwrap_or_default();
            let (local_sel, set_local_sel) = signal(selected_values);
            let k = key.clone();
            view! {
                <div class="pref-row__multiselect">
                    <For each=move || options.clone() key=|o| o.value.clone()
                        children=move |opt| {
                            let opt_val = opt.value.clone();
                            let is_checked = Signal::derive({
                                let ov = opt_val.clone();
                                move || local_sel.get().contains(&ov)
                            });
                            let k2 = k.clone();
                            let ov2 = opt_val.clone();
                            view! {
                                <label class="pref-row__multiselect-option">
                                    <input type="checkbox"
                                        checked=move || is_checked.get()
                                        on:change=move |ev| {
                                            let checked = event_target_checked(&ev);
                                            let item = ov2.clone();
                                            set_local_sel.update(|sel| {
                                                if checked { sel.push(item.clone()); }
                                                else { sel.retain(|x| x != &item); }
                                            });
                                            let k3 = k2.clone();
                                            let item2 = ov2.clone();
                                            let lv = live_values;
                                            leptos::task::spawn_local(async move {
                                                let _ = toggle_preference_select_item(
                                                    source_id, k3.clone(), item2, checked
                                                ).await;

                                                if let Ok(vals) = get_source_preferences(source_id).await {
                                                    let map: HashMap<_, _> = vals.into_iter().collect();
                                                    if let Some(v) = map.get(&k3) {
                                                        let v = v.clone();
                                                        lv.update(|m| { m.insert(k3, v); });
                                                    }
                                                }
                                            });
                                        }
                                    />
                                    <span>{opt.label.clone()}</span>
                                </label>
                            }
                        }
                    />
                </div>
            }.into_any()
        },

        PreferenceKind::MultiValueList { placeholder, item_label, .. } => {
            let ph = placeholder.unwrap_or_else(|| "Add item…".into());
            let items: Vec<String> =
                serde_json::from_str(&current_value).unwrap_or_default();
            let (list, set_list) = signal(items);
            let (new_item, set_new_item) = signal(String::new());
            let k_add = key.clone();
            let k_add_keydown = k_add.clone();
            let k_remove = key.clone();

            view! {
                <div class="pref-row__mvl">
                    <ul class="pref-row__mvl-list">
                        <For each=move || list.get() key=|i| i.clone()
                            children=move |item| {
                                let item_rm = item.clone();
                                let k = k_remove.clone();
                                view! {
                                    <li class="pref-row__mvl-item">
                                        <span class="pref-row__mvl-item-text">
                                            {item.clone()}
                                        </span>
                                        <button
                                            class="pref-row__mvl-remove"
                                            title="Remove"
                                            on:click=move |_| {
                                                let item2 = item_rm.clone();
                                                set_list.update(|l| l.retain(|x| x != &item2));
                                                let k2 = k.clone();
                                                let item3 = item_rm.clone();
                                                let lv = live_values;
                                                leptos::task::spawn_local(async move {
                                                    let _ = remove_preference_list_item(
                                                        source_id, k2.clone(), item3
                                                    ).await;
                                                    let encoded = serde_json::to_string(
                                                        &list.get_untracked()
                                                    ).unwrap();
                                                    lv.update(|m| { m.insert(k2, encoded); });
                                                });
                                            }
                                        >"×"</button>
                                    </li>
                                }
                            }
                        />
                    </ul>
                    <div class="pref-row__mvl-add">
                        <input
                            class="pref-row__mvl-input"
                            type="text"
                            placeholder=ph
                            prop:value=move || new_item.get()
                            on:input=move |ev| set_new_item.set(event_target_value(&ev))
                            on:keydown=move |ev| {
                                if ev.key() == "Enter" {
                                    let item = new_item.get_untracked().trim().to_string();
                                    if item.is_empty() { return; }
                                    set_new_item.set(String::new());
                                    set_list.update(|l| {
                                        if !l.contains(&item) { l.push(item.clone()); }
                                    });
                                    let k = k_add.clone();
                                    let lv = live_values;
                                    leptos::task::spawn_local(async move {
                                        let _ = append_preference_list_item(
                                            source_id, k.clone(), item
                                        ).await;
                                        let encoded = serde_json::to_string(
                                            &list.get_untracked()
                                        ).unwrap();
                                        lv.update(|m| { m.insert(k, encoded); });
                                    });
                                }
                            }
                        />
                        <button
                            class="pref-row__mvl-add-btn"
                            disabled=move || new_item.get().trim().is_empty()
                            on:click={
                                let k = k_add_keydown.clone();
                                move |_| {
                                    let item = new_item.get_untracked().trim().to_string();
                                    if item.is_empty() { return; }
                                    set_new_item.set(String::new());
                                    set_list.update(|l| {
                                        if !l.contains(&item) { l.push(item.clone()); }
                                    });
                                    let k2 = k.clone();
                                    let lv = live_values;
                                    leptos::task::spawn_local(async move {
                                        let _ = append_preference_list_item(
                                            source_id, k2.clone(), item
                                        ).await;
                                        let encoded = serde_json::to_string(
                                            &list.get_untracked()
                                        ).unwrap();
                                        lv.update(|m| { m.insert(k2, encoded); });
                                    });
                                }
                            }
                        >
                            {item_label.unwrap_or_else(|| "+ Add".into())}
                        </button>
                    </div>
                </div>
            }.into_any()
        },
    };

    view! {
        <div 
            class="pref-row" 
            class:pref-row--label=is_label
            style:display=move || if visible.get() { "" } else { "none" }
        >
            {if !is_label { Some(view! {
                <div class="pref-row__meta">
                    <label class="pref-row__title" for=key_for_label.clone()>
                        {title.clone()}
                    </label>
                    {description.clone().map(|d| view! {
                        <p class="pref-row__desc">{d}</p>
                    })}
                </div>
            })} else { None }}
            <div class="pref-row__control">
                {widget}
            </div>
        </div>
    }
}