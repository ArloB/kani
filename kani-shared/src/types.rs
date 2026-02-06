//! Shared data types for manga information.

use serde::{Deserialize, Serialize};

/// Basic information about a manga.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaInfo {
    /// Unique identifier for this manga within the source
    pub id: String,
    /// Title of the manga
    pub title: String,
    /// URL to the cover image
    pub cover_url: Option<String>,
    /// Brief description or summary
    pub description: Option<String>,
    /// Author name(s)
    pub author: Option<String>,
    /// Artist name(s)
    pub artist: Option<String>,
    /// Current publication status
    pub status: MangaStatus,
    /// Content tags/genres
    pub tags: Vec<String>,
}

/// Publication status of a manga.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MangaStatus {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
    #[default]
    Unknown,
}

/// A paginated list of manga results.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaList {
    /// List of manga in this page
    pub manga: Vec<MangaInfo>,
    /// Whether there are more pages available
    pub has_next_page: bool,
}

/// Information about a single chapter.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterInfo {
    /// Unique identifier for this chapter within the source
    pub id: String,
    /// Chapter number (can be decimal for sub-chapters)
    pub number: f64,
    /// Optional chapter title
    pub title: Option<String>,
    /// Volume number if available
    pub volume: Option<i32>,
    /// Scanlation group name
    pub scanlator: Option<String>,
    /// Upload/release date as Unix timestamp
    pub date_uploaded: Option<i64>,
    /// Language code (e.g., "en", "ja")
    pub language: String,
}

/// A list of chapters for a manga.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterList {
    /// All chapters for the manga
    pub chapters: Vec<ChapterInfo>,
}

/// A chapter with its page URLs, ready for download.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Chapter {
    /// Display name for the chapter (used for folder naming)
    pub chapter_name: String,
    /// List of pages in order
    pub pages: Vec<Page>,
}

/// A single page in a chapter.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Page {
    /// Page index (0-based)
    pub index: i32,
    /// URL to download the page image
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manga_info_serialization() {
        let manga = MangaInfo {
            id: "123".to_string(),
            title: "Test Manga".to_string(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
            description: Some("A test manga".to_string()),
            author: Some("Author Name".to_string()),
            artist: None,
            status: MangaStatus::Ongoing,
            tags: vec!["action".to_string(), "comedy".to_string()],
        };

        let json = serde_json::to_string(&manga).unwrap();
        let deserialized: MangaInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(manga, deserialized);
    }

    #[test]
    fn test_chapter_serialization() {
        let chapter = Chapter {
            chapter_name: "Chapter 1".to_string(),
            pages: vec![
                Page {
                    index: 0,
                    url: "https://example.com/1.jpg".to_string(),
                },
                Page {
                    index: 1,
                    url: "https://example.com/2.jpg".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&chapter).unwrap();
        let deserialized: Chapter = serde_json::from_str(&json).unwrap();
        assert_eq!(chapter, deserialized);
    }

    #[test]
    fn test_manga_status_default() {
        let status: MangaStatus = Default::default();
        assert_eq!(status, MangaStatus::Unknown);
    }
}
