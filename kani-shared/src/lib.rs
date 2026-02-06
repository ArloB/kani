//! Kani Shared - Shared types and traits for the Kani manga downloader.
//!
//! This crate contains:
//! - Shared data types (MangaInfo, Chapter, etc.) used across the host and WASM extensions
//! - The MangaExtension trait that all extensions must implement
//! - Host ABI definitions for WASM-to-host communication
//! - Error types for extension operations
//!
//! This is a lightweight crate with minimal dependencies, designed to be
//! usable from both WASM and native contexts.

pub mod extension;
pub mod host_abi;
pub mod types;

pub use extension::*;
pub use host_abi::*;
pub use types::*;
