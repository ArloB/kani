//! Browser-safe type mirrors for `kani_shared` types that appear in `#[server]` function signatures.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaListItem {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaList {
    pub manga: Vec<MangaListItem>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MangaStatus {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
    Unknown,
}

impl std::fmt::Display for MangaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MangaStatus::Ongoing => write!(f, "Ongoing"),
            MangaStatus::Completed => write!(f, "Completed"),
            MangaStatus::Hiatus => write!(f, "Hiatus"),
            MangaStatus::Cancelled => write!(f, "Cancelled"),
            MangaStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

impl From<MangaStatus> for i64 {
    fn from(status: MangaStatus) -> Self {
        match status {
            MangaStatus::Ongoing => 0,
            MangaStatus::Completed => 1,
            MangaStatus::Hiatus => 2,
            MangaStatus::Cancelled => 3,
            MangaStatus::Unknown => 4,
        }
    }
}

impl From<i64> for MangaStatus {
    fn from(status: i64) -> Self {
        match status {
            0 => MangaStatus::Ongoing,
            1 => MangaStatus::Completed,
            2 => MangaStatus::Hiatus,
            3 => MangaStatus::Cancelled,
            _ => MangaStatus::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaInfo {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub status: MangaStatus,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Chapter {
    pub id: String,
    pub title: Option<String>,
    pub number: f64,
    pub volume: Option<i64>,
    pub language: String,
    pub scanlator: Option<String>,
    pub date_uploaded: Option<i64>,
    #[serde(default)]
    pub download_status: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterList {
    pub chapters: Vec<Chapter>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveChapterStatus {
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
    Deleted,
}
