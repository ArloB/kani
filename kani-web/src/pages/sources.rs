use crate::server_fns::fetch_sources;
use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn Sources() -> impl IntoView {
    let sources = Resource::new(|| (), |_| fetch_sources());

    view! {
        <h1>"Sources"</h1>
        <div class="source-list">
            <Suspense fallback=move || view! { <p>"Loading sources..."</p> }>
                {move || {
                    sources.get().map(|res| match res {
                        Ok(sources) => view! {
                            <div class="sources-grid" style="display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 1rem;">
                                <For
                                    each=move || sources.clone()
                                    key=|source| source.id
                                    children=move |source| {
                                        view! {
                                            <div class="source-card">
                                                <h3>{source.name}</h3>
                                                <p>"Version: " {source.version}</p>
                                                <A href=format!("/source/{}", source.id)>"Browse"</A>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        }.into_any(),
                        Err(e) => view! { <p class="error">"Error loading sources: " {e.to_string()}</p> }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}
