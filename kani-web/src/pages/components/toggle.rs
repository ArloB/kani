use leptos::prelude::*;

#[component]
pub fn Toggle(
    checked: Signal<bool>,
    on_change: impl Fn(bool) + 'static + Send + Sync,
    #[prop(optional)]
    extra_class: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let on_change_sv = StoredValue::new(on_change);
    let class = move || {
        let base = "kani-toggle";
        match extra_class {
            Some(extra) => format!("{base} {extra}"),
            None => base.to_string(),
        }
    };

    view! {
        <label class=class>
            <input
                class="kani-toggle__input"
                type="checkbox"
                checked=move || checked.get()
                on:change=move |ev| {
                    let v = event_target_checked(&ev);
                    on_change_sv.with_value(|f| f(v));
                }
            />
            <span class="kani-toggle__track"></span>
            {children()}
        </label>
    }
}
