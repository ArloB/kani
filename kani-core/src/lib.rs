//! Kani Core - Manga downloader core functionality.
//!
//! This crate provides the core business logic for the Kani manga downloader,
//! including WASM extension hosting, download management, and source handling.

pub mod cbz;
pub mod comic_info;
pub mod downloader;
pub mod error;
pub mod evaluator;
pub mod file_storage;
pub mod http;
pub mod network;
pub mod source_manager;
pub mod sources;
pub mod utilities;
pub mod v8_process;
pub mod wasm;

pub use error::Error;

/// Runtime toggle: when `false`, HTTP request log lines are suppressed.
/// Initialised from settings at startup and updated on settings change.
pub static HTTP_LOGGING_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

// Re-export wasmtime for downstream crates
pub use wasmtime;

/// Host-side WIT-generated `PreferenceSpec` (with serde derives).
pub use wasm::kani::extension::types::PreferenceSpec;

/// Host-side WIT-generated `FilterList` (with serde derives).
pub use wasm::kani::extension::types::FilterList as WitFilterList;
