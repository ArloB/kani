use leptos::prelude::*;

#[component]
pub fn CoverImage(url: Option<String>, alt: String) -> impl IntoView {
    view! {
        <div class="cover">
            {match url {
                Some(url) => view! { <img src=url alt=alt /> }.into_any(),
                None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
            }}
        </div>
    }
}