use crate::server_fns::{get_library, proxy_url};
use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn Library() -> impl IntoView {
    let (page, set_page) = signal(1);

    let library_manga = Resource::new(
        move || page.get(),
        move |p| async move { get_library(p).await },
    );

    view! {
        <div class="library-page">
            <h1>"My Library"</h1>
            <Suspense fallback=move || view! { <p>"Loading library..."</p> }>
                {move || library_manga.get().map(|res| match res {
                    Ok(mangas) => {
                        if mangas.is_empty() {
                            view! { <p>"Your library is empty. Go add some manga!"</p> }.into_any()
                        } else {
                            let list_pagination = mangas.clone();
                            view! {
                                <div class="manga-grid">
                                    <For
                                        each=move || mangas.clone()
                                        key=|(m, _)| m.id.clone()
                                        children=move |(manga, base_url)| view! {
                                            <div class="manga-card">
                                                <A href=format!("/manga/{}", manga.id)>
                                                    <div class="cover">
                                                        {match manga.cover_url {
                                                            Some(url) => {
                                                                let src = proxy_url(&url, &base_url);
                                                                view! { <img src=src alt=manga.title.clone() /> }.into_any()
                                                            },
                                                            None => view! { <div class="no-cover">"No Cover"</div> }.into_any(),
                                                        }}
                                                    </div>
                                                    <div class="title">{manga.title}</div>
                                                </A>
                                            </div>
                                        }
                                    />
                                </div>
                                
                                <Show when=move || !list_pagination.is_empty() fallback=|| ()>
                                    <div class="pagination">
                                        <button on:click=move |_| set_page.update(|p| *p = (*p - 1).max(1)) disabled=move || page.get() <= 1>"Prev"</button>
                                        <span>" Page " {page} </span>
                                        <button on:click=move |_| set_page.update(|p| *p += 1)>"Next"</button>
                                    </div>
                                </Show>
                            }.into_any()
                        }
                    },
                    Err(e) => view! { <p class="error">"Error: " {e.to_string()}</p> }.into_any()
                })}
            </Suspense>
        </div>
    }
}