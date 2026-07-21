//! Kani Core - Manga downloader core functionality.
//!
//! This crate provides the core business logic for the Kani manga downloader,
//! including WASM extension hosting, download management, and source handling.

pub mod archive;
pub mod cache;
pub mod cbz;
pub mod comic_info;
pub mod downloader;
pub mod error;
pub mod evaluator;
pub mod file_storage;
pub mod http;
pub mod image_transform;
pub mod manifest;
pub mod network;
pub mod option_set_fetcher;
pub mod quality;
pub mod scripting;
pub mod sources;
pub mod utilities;
pub mod v8_process;
pub mod wasm;

pub use error::Error;

/// Runtime toggle: when `false`, HTTP request log lines are suppressed.
/// Initialised from settings at startup and updated on settings change.
pub static HTTP_LOGGING_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Runtime opt-in: when `false`, no error events are reported upstream.
/// Defaults to off so a build with reporting compiled in stays silent until
/// an operator opts in. Initialised from settings at startup and updated on
/// settings change.
pub static ERROR_REPORTING_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub use wasmtime;

/// Host-side WIT-generated `PreferenceSpec` (with serde derives).
pub use wasm::kani::extension::types::PreferenceSpec;

/// Host-side WIT-generated `FilterList` (with serde derives).
pub use wasm::kani::extension::types::FilterList as WitFilterList;

#[cfg(test)]
mod runtime_toggle_tests {
    #![allow(clippy::unwrap_used)]
    use std::sync::atomic::Ordering;

    #[test]
    fn error_reporting_is_off_until_explicitly_enabled() {
        assert!(
            !super::ERROR_REPORTING_ENABLED.load(Ordering::Relaxed),
            "the reporting gate must default to off so a build with the feature \
             compiled in stays silent until an operator opts in"
        );
    }
}
