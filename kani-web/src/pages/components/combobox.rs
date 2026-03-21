use leptos::{prelude::*, web_sys};

const ITEM_H: usize = 36;
const VISIBLE_COUNT: usize = 8;

#[component]
pub fn Combobox(
    options: Signal<Vec<(i64, String)>>,
    value: Signal<Option<i64>>,
    on_change: impl Fn(Option<i64>) + 'static + Send + Sync,
    #[prop(into)]
    placeholder: String,
) -> impl IntoView {
    let (input_text, set_input_text) = signal(String::new());
    let (open, set_open) = signal(false);
    let (highlighted, set_highlighted) = signal(0usize);
    let (scroll_top, set_scroll_top) = signal(0.0_f64);
    let dropdown_ref = NodeRef::<leptos::html::Div>::new();
    
    let (is_typing, set_is_typing) = signal(false);

    let on_change_stored = StoredValue::new(on_change);

    Effect::new(move |_| {
        let current_val = value.get();

        if is_typing.get_untracked() {
            return;
        }

        match current_val {
            None => {
                if !input_text.get_untracked().is_empty() {
                    set_input_text.set(String::new());
                }
            }
            Some(id) => {
                if let Some((_, name)) = options.get_untracked().into_iter().find(|(i, _)| *i == id)
                && input_text.get_untracked() != name {
                    set_input_text.set(name);
                }
            }
        }
    });

    let filtered = Memo::new(move |_| {
        let query = input_text.get().to_ascii_lowercase();
        options
            .get()
            .into_iter()
            .filter(|(_, name)| name.to_ascii_lowercase().contains(&query))
            .collect::<Vec<_>>()
    });

    let total_h = Memo::new(move |_| filtered.get().len() * ITEM_H);

    let win_start = Memo::new(move |_| {
        let raw = (scroll_top.get() / ITEM_H as f64).floor() as usize;
        raw.saturating_sub(2)
    });

    let visible_items = Memo::new(move |_| {
        let start = win_start.get();
        let items = filtered.get();
        let end = (start + VISIBLE_COUNT + 4).min(items.len());
        items
            .into_iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect::<Vec<_>>()
    });

    let sync_scroll = move |target_top: usize| {
        set_scroll_top.set(target_top as f64);
        if let Some(el) = dropdown_ref.get() {
            el.set_scroll_top(target_top as i32);
        }
    };

    let on_key = move |ev: web_sys::KeyboardEvent| {
        let n = filtered.get().len();
        let container_h = VISIBLE_COUNT * ITEM_H;

        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                set_open.set(true);
                let next = (highlighted.get() + 1).min(n.saturating_sub(1));
                set_highlighted.set(next);
                let bot = (next + 1) * ITEM_H;
                let cur = scroll_top.get() as usize;
                if bot > cur + container_h {
                    sync_scroll(bot - container_h);
                }
            }
            "ArrowUp" => {
                ev.prevent_default();
                let prev = highlighted.get().saturating_sub(1);
                set_highlighted.set(prev);
                let top = prev * ITEM_H;
                let cur = scroll_top.get() as usize;
                if top < cur {
                    sync_scroll(top);
                }
            }
            "Enter" => {
                ev.prevent_default();
                if open.get() {
                    let items = filtered.get();
                    if let Some((_, (id, name))) =
                        items.into_iter().enumerate().nth(highlighted.get())
                    {
                        set_is_typing.set(false);
                        set_input_text.set(name);
                        set_open.set(false);
                        set_scroll_top.set(0.0);
                        on_change_stored.with_value(|cb| cb(Some(id)));
                    }
                }
            }
            "Escape" | "Tab" => {
                set_open.set(false);
            }
            _ => {}
        }
    };

    view! {
        <div class="combobox" class:combobox--open=open>
            <div class="combobox__field">
                <input
                    type="text"
                    class="combobox__input"
                    placeholder=placeholder
                    prop:value=move || input_text.get()
                    autocomplete="off"
                    spellcheck="false"
                    on:input=move |ev| {
                        set_is_typing.set(true);
                        set_input_text.set(event_target_value(&ev));
                        set_open.set(true);
                        set_highlighted.set(0);
                        set_scroll_top.set(0.0);
                        if let Some(el) = dropdown_ref.get() {
                            el.set_scroll_top(0);
                        }
                        on_change_stored.with_value(|cb| cb(None));
                    }
                    on:focus=move |_| set_open.set(true)
                    on:blur=move |_| {
                        set_is_typing.set(false);
                        set_timeout(
                            move || set_open.set(false),
                            std::time::Duration::from_millis(150),
                        );
                    }
                    on:keydown=on_key
                />
                <Show when=move || !input_text.get().is_empty()>
                    <button
                        class="combobox__clear"
                        tabindex="-1"
                        aria-label="Clear"
                        on:mousedown=move |ev: web_sys::MouseEvent| {
                            ev.prevent_default();
                            set_is_typing.set(false);
                            set_input_text.set(String::new());
                            set_open.set(false);
                            set_scroll_top.set(0.0);
                            on_change_stored.with_value(|cb| cb(None));
                        }
                    >
                        "×"
                    </button>
                </Show>
                <span class="combobox__chevron" aria-hidden="true">"▾"</span>
            </div>

            <Show when=move || open.get() && !filtered.get().is_empty()>
                <div
                    class="combobox__dropdown"
                    node_ref=dropdown_ref
                    style=move || format!(
                        "height: {}px; overflow-y: auto;",
                        (VISIBLE_COUNT * ITEM_H).min(total_h.get()),
                    )
                    on:scroll=move |_| {
                        if let Some(el) = dropdown_ref.get() {
                            set_scroll_top.set(el.scroll_top() as f64);
                        }
                    }
                >
                    <div style=move || format!("height: {}px; position: relative;", total_h.get())>
                        {move || {
                            visible_items.get().into_iter().map(|(abs_idx, (id, name))| {
                                let name_on_click = name.clone();
                                
                                view! {
                                    <div
                                        class="combobox__option"
                                        class:combobox__option--highlighted=move || {
                                            highlighted.get() == abs_idx
                                        }
                                        style=format!(
                                            "position: absolute; top: {}px; height: {}px; \
                                             line-height: {}px; width: 100%;",
                                            abs_idx * ITEM_H,
                                            ITEM_H,
                                            ITEM_H,
                                        )
                                        on:mousedown=move |ev: web_sys::MouseEvent| {
                                            ev.prevent_default();
                                            set_is_typing.set(false);
                                            set_input_text.set(name_on_click.clone());
                                            set_open.set(false);
                                            set_scroll_top.set(0.0);
                                            on_change_stored.with_value(|cb| cb(Some(id)));
                                        }
                                        on:mouseover=move |_| set_highlighted.set(abs_idx)
                                    >
                                        {name}
                                    </div>
                                }
                            }).collect_view()
                        }}
                    </div>
                </div>
            </Show>
        </div>
    }
}