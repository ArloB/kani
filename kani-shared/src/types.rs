//! Shared data types for manga information.

use serde::{Deserialize, Serialize};

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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
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

#[allow(clippy::derivable_impls)]
impl Default for MangaStatus {
    fn default() -> Self {
        MangaStatus::Unknown
    }
}

impl From<i64> for MangaStatus {
    fn from(value: i64) -> Self {
        match value {
            0 => MangaStatus::Ongoing,
            1 => MangaStatus::Completed,
            2 => MangaStatus::Hiatus,
            3 => MangaStatus::Cancelled,
            _ => MangaStatus::Unknown,
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

/// Events emitted during chapter/page downloads, shared between kani-core and kani-web.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadProgressEvent {
    ChapterStarted {
        chapter_id: i64,
        chapter_name: String,
        total_pages: usize,
    },

    PageCompleted {
        chapter_id: i64,
        chapter_name: String,
        page_index: i32,
    },

    ChapterCompleted {
        chapter_id: i64,
        chapter_name: String,
        successful_pages: usize,
        failed_pages: usize,
    },

    ChapterFailed {
        chapter_id: i64,
        chapter_name: String,
        error: String,
    },

    ChapterCancelled {
        chapter_id: i64,
        chapter_name: String,
    },
}
