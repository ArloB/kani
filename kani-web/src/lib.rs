pub const KANI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod app;
pub mod auth;
pub mod cache;
pub mod csrf;
pub mod error;
pub mod etag;
pub mod i18n;
pub mod idempotency;
pub mod install_gating;
pub mod logging;
pub mod metrics;
pub mod middleware;
pub mod models;
pub mod opds;
pub mod openapi;
pub mod permissions;
pub mod proxy;
pub mod rate_limit;
pub mod rate_limit_key;
pub mod repo_keys;
pub mod rest;
pub mod session_touch;
pub mod state;
pub mod types;
pub mod utils;

pub use kani_core::HTTP_LOGGING_ENABLED;

pub static SOURCE_INSTALL_ALLOWED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);
