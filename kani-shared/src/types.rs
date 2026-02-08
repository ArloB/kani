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
    pub authors: Vec<String>,
    /// Artist name(s)
    pub artists: Vec<String>,
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
    pub has_next_page: bool,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PreferenceList {
    pub preferences: Vec<Preference>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Preference {
    pub name: String,
    pub preference_type: PreferenceType,
    pub options: Vec<PreferenceOption>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum PreferenceType {
    Select,
    Checkbox,
    TextInput,
    Sort,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PreferenceOption {
    pub name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FilterList {
    pub filters: Vec<Filter>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Filter {
    pub name: String,
    pub filter_type: FilterType,
    pub options: Vec<FilterOption>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum FilterType {
    Select,
    Checkbox,
    TextInput,
    Sort,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FilterOption {
    pub name: String,
    pub value: String,
}
