//! Kani application layer — business logic and orchestration.
//!
//! This crate sits between the HTTP layer (`kani-web`) and the core WASM/download
//! infrastructure (`kani-core`). It owns all database-backed orchestration that is
//! not specific to any particular interface (web, CLI, TUI, …).

pub mod cache;
pub mod error;
pub mod events;
pub mod models;
pub mod service;
pub mod utils;

pub use error::ServiceError;
pub use models::{
    ChapterPageManifest, ChapterRow, LibraryManga, LocalMangaDetails, Manga, PageInfo,
};
pub use service::{AppService, chapter_name};
