use crate::pages::downloads::DownloadProgress;
use crate::pages::home::Home;
use crate::pages::manga_details::MangaDetails;
use crate::pages::source_details::SourceDetails;
use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/kani-web.css"/>
        <Title text="Kani Manga Reader"/>
        <div id="root">
            <Router>
                <header>
                    <A href="/">"Kani"</A>
                </header>
                <main class="container">
                    <Routes fallback=|| view! { <h1>"Not Found"</h1> }>
                        <Route path=path!("/") view=Home/>
                        <Route path=path!("/source/:id") view=SourceDetails/>
                        <Route path=path!("/source/:id/manga/:manga_id") view=MangaDetails/>
                    </Routes>
                </main>
            </Router>
            <DownloadProgress/>
        </div>
    }
}
