//! Kani Core - Manga downloader core functionality.
//!
//! This crate provides the core business logic for the Kani manga downloader,
//! including WASM extension hosting, download management, and source handling.

pub mod downloader;
pub mod error;
pub mod file_storage;
pub mod http;
pub mod network;
pub mod sanitize;
pub mod source_manager;
pub mod sources;
pub mod wasm;

pub use error::Error;

// Re-export wasmtime for downstream crates
pub use wasmtime;
