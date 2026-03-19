use leptos::{either::Either, prelude::*};
use leptos_router::components::A;
use crate::server_fns::fetch_sources;
use crate::types::Source;

#[component]
pub fn Sources() -> impl IntoView {
    let sources = Resource::new(|| (), |_| fetch_sources());

    view! {
        <h1>"Sources"</h1>
        <div class="source-list">
            {move || match sources.get() {
                None => Either::Left(view! {
                    <div class="skeleton-source-grid">
                        {(0..6).map(|_| view! {
                            <div class="skeleton-source-card">
                                <div class="skeleton-source-card__meta">
                                    <div class="skeleton-source-card__name"></div>
                                    <div class="skeleton-source-card__meta"></div>
                                </div>
                                <div class="skeleton-source-card__btn"></div>
                            </div>
                        }).collect::<Vec<_>>()}
                    </div>
                }),

                Some(result) => {
                    let display: Vec<Source> = result
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|s| s.enabled)
                        .collect();

                    Either::Right(if display.is_empty() {
                        view! {
                            <p class="empty">"No sources found."</p>
                        }.into_any()
                    } else {
                        view! {
                            <div class="sources-grid">
                                <For
                                    each=move || display.clone()
                                    key=|s| s.id
                                    children=move |source| view! {
                                        <div class="source-card">
                                            <h3>{source.name.clone()}</h3>
                                            <p>"Version: " {source.version.clone()}</p>
                                            <A href=format!("/source/{}", source.id)>
                                                "Browse"
                                            </A>
                                        </div>
                                    }
                                />
                            </div>
                        }.into_any()
                    })
                }
            }}
        </div>
    }
}