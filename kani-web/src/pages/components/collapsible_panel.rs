use leptos::prelude::*;

#[derive(Clone, PartialEq, Default)]
pub enum CollapsibleVariant {
    #[default]
    Panel,
    Section,
}

#[component]
pub fn CollapsiblePanel(
    #[prop(into)] label: TextProp, 
    #[prop(default = false)] open: bool,
    #[prop(default = CollapsibleVariant::Panel)] variant: CollapsibleVariant,
    children: ChildrenFn,
) -> impl IntoView {
    let (is_open, set_open) = signal(open);
    
    let children = StoredValue::new(children);

    match variant {
        CollapsibleVariant::Panel => view! {
            <div class="collapsible-panel">
                <button
                    class="collapsible-panel__toggle"
                    on:click=move |_| set_open.update(|v| *v = !*v)
                >
                    <span class="collapsible-panel__label">{move || label.get()}</span>
                    <span class="collapsible-panel__chevron">
                        {move || if is_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>
                <Show when=move || is_open.get() fallback=|| ()>
                    <div class="collapsible-panel__body">
                        {(children.get_value())()}
                    </div>
                </Show>
            </div>
        }.into_any(),
        CollapsibleVariant::Section => view! {
            <section class="settings-section">
                <button
                    class="settings-section__toggle"
                    on:click=move |_| set_open.update(|v| *v = !*v)
                >
                    <span class="settings-section__title">{move || label.get()}</span>
                    <span class="settings-section__chevron">
                        {move || if is_open.get() { "▾" } else { "▸" }}
                    </span>
                </button>
                <Show when=move || is_open.get() fallback=|| ()>
                    <div class="settings-section__body">
                        {(children.get_value())()}
                    </div>
                </Show>
            </section>
        }.into_any(),
    }
}