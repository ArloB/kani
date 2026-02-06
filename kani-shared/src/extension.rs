//! Extension trait definitions.
//!
//! These traits define the interface that WASM extensions must implement
//! to provide manga source functionality.

use crate::types::{Chapter, ChapterList, MangaInfo, MangaList};

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
///
/// This defines the core functionality required to fetch manga data
/// from a source. Each method corresponds to a WASM export function.
pub trait MangaExtension {
    /// Get the name of this source (e.g., "MangaDex", "MangaSee").
    fn name(&self) -> &str;

    /// Get popular/trending manga.
    ///
    /// # Arguments
    /// * `page` - Page number (1-indexed)
    fn get_popular_manga(&self, page: i32) -> ExtensionResult<MangaList>;

    /// Search for manga by query.
    ///
    /// # Arguments
    /// * `query` - Search query string
    /// * `page` - Page number (1-indexed)
    fn search_manga(&self, query: &str, page: i32) -> ExtensionResult<MangaList>;

    /// Get detailed information about a specific manga.
    ///
    /// # Arguments
    /// * `manga_id` - The manga's unique identifier
    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo>;

    /// Get all chapters for a manga.
    ///
    /// # Arguments
    /// * `manga_id` - The manga's unique identifier
    fn get_chapter_list(&self, manga_id: &str) -> ExtensionResult<ChapterList>;

    /// Get page URLs for a chapter for downloading.
    ///
    /// # Arguments
    /// * `manga_id` - The manga's unique identifier
    /// * `chapter_id` - The chapter's unique identifier
    fn get_pages(&self, manga_id: &str, chapter_id: &str) -> ExtensionResult<Chapter>;
}

/// Metadata about a source extension.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionMetadata {
    /// Unique identifier for this extension
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Version string (semver recommended)
    pub version: String,
    /// Base URL of the source
    pub base_url: String,
    /// Language code (e.g., "en", "multi")
    pub language: String,
    /// Whether the source supports NSFW content
    pub nsfw: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_error_display() {
        let err = ExtensionError::NetworkError("connection refused".to_string());
        assert_eq!(err.to_string(), "Network error: connection refused");

        let err = ExtensionError::RateLimited;
        assert_eq!(err.to_string(), "Rate limited");
    }
}
