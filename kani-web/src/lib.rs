pub mod app;
pub mod events;
pub mod pages;
pub mod server_fns;
pub mod types;
pub mod utils;
pub mod permissions;

#[cfg(feature = "ssr")]
pub mod error;
#[cfg(feature = "ssr")]
pub mod models;
#[cfg(feature = "ssr")]
pub mod rest;
#[cfg(feature = "ssr")]
pub mod state;
#[cfg(feature = "ssr")]
pub mod cache;
#[cfg(feature = "ssr")] 
pub mod proxy;
#[cfg(feature = "ssr")]
pub mod markdown;
#[cfg(feature = "ssr")]
pub mod auth;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use app::App;
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Debug);
    leptos::mount::hydrate_body(App);
}
