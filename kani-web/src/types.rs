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
    CompletedHidden,
    Failed(String),
    Cancelled,
    Deleted,
}

/// Live per-chapter progress tracking for downloads.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterProgress {
    pub id: i64,
    pub name: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub status: LiveChapterStatus,
}

impl ChapterProgress {
    pub fn completion_pct(&self) -> f64 {
        if self.total_pages == 0 {
            return 100.0;
        }
        (self.completed_pages) as f64 / self.total_pages as f64 * 100.0
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveDownloadStatus {
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Deserialize)]
pub struct ActiveDownloadState {
    pub chapter_id:       i64,
    pub chapter_name:     String,
    pub total_pages:      usize,
    pub completed_pages:  usize,
    pub status:           ActiveDownloadStatus,
}

impl From<ActiveDownloadStatus> for LiveChapterStatus {
    fn from(s: ActiveDownloadStatus) -> Self {
        match s {
            ActiveDownloadStatus::InProgress  => LiveChapterStatus::InProgress,
            ActiveDownloadStatus::Completed   => LiveChapterStatus::Completed,
            ActiveDownloadStatus::Failed(e)   => LiveChapterStatus::Failed(e),
            ActiveDownloadStatus::Cancelled   => LiveChapterStatus::Cancelled,
        }
    }
}

impl From<ActiveDownloadState> for ChapterProgress {
    fn from(s: ActiveDownloadState) -> Self {
        ChapterProgress {
            id:              s.chapter_id,
            name:            s.chapter_name,
            total_pages:     s.total_pages,
            completed_pages: s.completed_pages,
            status:          s.status.into(),
        }
    }
}