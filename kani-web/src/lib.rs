pub mod app;
pub mod events; // DownloadProgressEvent mirror — no kani-shared dep, safe for both targets
pub mod pages;
pub mod server_fns; // replaces api.rs
pub mod types;     // browser-safe mirrors of kani_shared types

#[cfg(feature = "ssr")]
pub mod error;
#[cfg(feature = "ssr")]
pub mod models;
#[cfg(feature = "ssr")]
pub mod rest;
#[cfg(feature = "ssr")]
pub mod state;

/// WASM entry point — only compiled for the `hydrate` browser bundle.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use app::App;
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Debug);
    leptos::mount::hydrate_body(App);
}
