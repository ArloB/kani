//! Server-specific models (database entities and API request/response types).

use serde::{Deserialize, Serialize};

// Re-export shared types for convenience
pub use kani_shared::Chapter;

#[derive(Serialize, Deserialize, Clone, Debug, sqlx::FromRow)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
}

#[derive(Deserialize, Debug)]
pub struct CreateSource {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct UpdateSource {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct FetchWasmRequest {
    pub url: String,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Settings {
    pub flaresolverr_url: String,
    pub library_path: std::path::PathBuf,
    pub wasm_storage_path: std::path::PathBuf,
    pub concurrent_page_downloads: i64,
    pub chapter_queue_size: i64,
    pub max_retries: i64,
    pub initial_retry_delay_ms: i64,
}

#[derive(Deserialize, Debug)]
pub struct SearchMangaRequest {
    pub query: String,
}

#[derive(Deserialize, Debug)]
pub struct ProxyQuery {
    pub url: String,
    pub referer: String,
}
