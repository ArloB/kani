use leptos::prelude::*;

#[component]
pub fn Pagination(
    page: ReadSignal<i32>,
    set_page: WriteSignal<i32>,
    has_next: Signal<bool>,
) -> impl IntoView {
    view! {
        <div class="pagination">
            <button
                on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1))
                disabled=move || page.get() <= 1
            >"← Prev"</button>
            <span>"Page " {page}</span>
            <button
                on:click=move |_| set_page.update(|p| *p += 1)
                disabled=move || !has_next.get()
            >"Next →"</button>
        </div>
    }
}