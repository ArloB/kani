//! Shared data types for manga information.

use serde::{Deserialize, Serialize};

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
pub struct MangaListItem {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
}

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MangaList {
    pub manga: Vec<MangaListItem>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterInfo {
    pub id: String,
    pub number: f64,
    pub title: Option<String>,
    pub volume: Option<i32>,
    pub scanlator: Option<String>,
    pub date_uploaded: Option<i64>,
    pub language: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterList {
    pub chapters: Vec<ChapterInfo>,
    pub has_next_page: bool,
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Chapter {
    pub chapter_name: String,
    pub pages: Vec<Page>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Page {
    pub index: i32,
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
