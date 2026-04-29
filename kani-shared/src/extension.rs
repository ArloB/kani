//! Extension trait definitions.
//!
//! These traits define the interface that WASM extensions must implement
//! to provide manga source functionality.

use crate::{
    types::ActiveFilter,
    wit_types::{Chapter, ChapterList, ChapterSortOption, FilterList, MangaInfo, MangaList, PreferenceSpec},
};

/// Result type for extension operations.
pub type ExtensionResult<T> = Result<T, ExtensionError>;

/// Errors that can occur during extension operations.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionError {
    /// Network request failed
    NetworkError(String),
    /// Failed to parse response
    ParseError(String),
    /// Resource not found
    NotFound(String),
    /// Rate limited by the source
    RateLimited,
    /// Generic error with message
    Other(String),
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ExtensionError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            ExtensionError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ExtensionError::RateLimited => write!(f, "Rate limited"),
            ExtensionError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ExtensionError {}

/// Trait that all manga source extensions must implement.
pub trait MangaExtension {
    fn name(&self) -> &str;

    fn get_popular_manga(&self, page: i32, page_size: i32, filters: &[ActiveFilter]) -> ExtensionResult<MangaList>;

    fn search_manga(&self, query: &str, page: i32, page_size: i32, filters: &[ActiveFilter]) -> ExtensionResult<MangaList>;

    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo>;

    fn get_chapter_list(
        &self,
        manga_id: &str,
        page: i32,
        page_size: Option<i32>,
        sort: Option<String>,
    ) -> ExtensionResult<ChapterList>;

    fn get_pages(&self, manga_id: &str, chapter_id: &str) -> ExtensionResult<Chapter>;

    /// Returns the sort options this extension supports for its chapter list.
    fn get_chapter_sort_list(&self) -> ExtensionResult<Vec<ChapterSortOption>>;

    /// Returns the available filters for search/browse.
    fn get_filter_list(&self) -> ExtensionResult<FilterList>;

    /// Returns the extension's preference definitions.
    /// The host stores and serves the values; extensions read them via `host_abi::prefs`.
    fn get_preferences(&self) -> ExtensionResult<Vec<PreferenceSpec>>;

    /// Returns the canonical upstream URL for a manga, suitable for opening in a browser.
    /// Defaults to an error; extensions with a straightforward URL scheme should override.
    fn get_url(&self, _manga_id: &str) -> ExtensionResult<String> {
        Err(ExtensionError::Other("get_url not implemented for this source".into()))
    }
}

/// Metadata about a source extension.
/// Used on the host side for deserialization and caching.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(serde::Serialize, serde::Deserialize))]
pub struct ExtensionMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub language: String,
    pub nsfw: bool,
    pub unrestricted_http: bool,
}
