use leptos::{prelude::*, either::Either};
use leptos_router::components::A;
use crate::{
  server_fns::fetch_sources,
  pages::components::permission_handlers::RequirePermission,
};

#[component]
pub fn Sources() -> impl IntoView {
    let sources = Resource::new(|| (), |_| fetch_sources());

    view! {
      <RequirePermission permission="source:browse">
        <h1>"Sources"</h1>
        <div class="source-list">
            <Suspense fallback=move || view! { 
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
            }>
                {move || {
                    sources.get().map(|res| {
                        let display_sources: Vec<_> = res
                            .ok()
                            .unwrap_or_default()
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
                    })
                }}
            </Suspense>
        </div>
      </RequirePermission>
    }
}