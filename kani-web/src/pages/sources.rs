use leptos::{prelude::*, either::Either};
use leptos_router::components::A;
use crate::server_fns::fetch_sources;

#[component]
pub fn Sources() -> impl IntoView {
    let sources = Resource::new(|| (), |_| fetch_sources());

    view! {
        <h1>"Sources"</h1>
        <div class="source-list">
            <Suspense fallback=move || view! { <p class="spinner">"Loading sources..."</p> }>
                {move || {
                    let loaded_sources = sources.get()
                        .and_then(|res| res.ok())
                        .unwrap_or_default();

                    let display_sources: Vec<_> = loaded_sources
                        .into_iter()
                        .filter(|s| s.enabled)
                        .collect();

                    if display_sources.is_empty() {
                        Either::Left(view! { 
                            <p class="empty">"No sources found."</p> 
                        })
                    } else {
                        Either::Right(view! {
                            <div class="sources-grid">
                                <For
                                    each=move || display_sources.clone()
                                    key=|source| source.id
                                    children=move |source| {
                                        view! {
                                            <div class="source-card">
                                                <h3>{source.name.clone()}</h3>
                                                <p>"Version: " {source.version.clone()}</p>
                                                <A href=format!("/source/{}", source.id)>"Browse"</A>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        })
                    }
                }}
            </Suspense>
        </div>
    }
}