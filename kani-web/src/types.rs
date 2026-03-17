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
    #[serde(default)]
    pub description_html: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChapterSortOrder {
    #[default]
    ChapterDesc,
    ChapterAsc,
    UploadedDesc,
    UploadedAsc,
    VolumeDesc,
    VolumeAsc,
    LanguageAsc,
    LanguageDesc,
    ScanlatorAsc,
    ScanlatorDesc,
}

impl ChapterSortOrder {
    pub fn to_sql_order(&self) -> &'static str {
        match self {
            Self::ChapterDesc   => "c.chapter_number DESC, c.id DESC",
            Self::ChapterAsc    => "c.chapter_number ASC, c.id ASC",
            Self::UploadedDesc  => "c.uploaded_at DESC, c.chapter_number DESC",
            Self::UploadedAsc   => "c.uploaded_at ASC, c.chapter_number ASC",
            Self::VolumeDesc    => "c.volume DESC NULLS LAST, c.chapter_number DESC",
            Self::VolumeAsc     => "c.volume ASC NULLS FIRST, c.chapter_number ASC",
            Self::LanguageAsc   => "c.language ASC NULLS LAST, c.chapter_number DESC",
            Self::LanguageDesc  => "c.language DESC NULLS LAST, c.chapter_number DESC",
            Self::ScanlatorAsc  => "c.scanlator ASC NULLS LAST, c.chapter_number DESC",
            Self::ScanlatorDesc => "c.scanlator DESC NULLS LAST, c.chapter_number DESC",
        }
    }

    pub fn to_select_value(&self) -> &'static str {
        match self {
            Self::ChapterDesc   => "chapter_desc",
            Self::ChapterAsc    => "chapter_asc",
            Self::UploadedDesc  => "uploaded_desc",
            Self::UploadedAsc   => "uploaded_asc",
            Self::VolumeDesc    => "volume_desc",
            Self::VolumeAsc     => "volume_asc",
            Self::LanguageAsc   => "language_asc",
            Self::LanguageDesc  => "language_desc",
            Self::ScanlatorAsc  => "scanlator_asc",
            Self::ScanlatorDesc => "scanlator_desc",
        }
    }

    pub fn from_select_value(s: &str) -> Self {
        match s {
            "chapter_asc"    => Self::ChapterAsc,
            "uploaded_desc"  => Self::UploadedDesc,
            "uploaded_asc"   => Self::UploadedAsc,
            "volume_desc"    => Self::VolumeDesc,
            "volume_asc"     => Self::VolumeAsc,
            "language_asc"   => Self::LanguageAsc,
            "language_desc"  => Self::LanguageDesc,
            "scanlator_asc"  => Self::ScanlatorAsc,
            "scanlator_desc" => Self::ScanlatorDesc,
            _                => Self::ChapterDesc,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MangaSortOrder {
    #[default]
    NameDesc,
    NameAsc,
    UpdatedDesc,
    UpdatedAsc,
    AddedDesc,
    AddedAsc,
}

impl MangaSortOrder {
    pub fn to_sql_order(&self) -> &'static str {
        match self {
            Self::NameDesc    => "m.name DESC, m.id DESC",
            Self::NameAsc     => "m.name ASC, m.id ASC",
            Self::UpdatedDesc => "m.updated_at DESC, m.name ASC",
            Self::UpdatedAsc  => "m.updated_at ASC, m.name ASC",
            Self::AddedDesc   => "m.created_at DESC, m.name ASC",
            Self::AddedAsc    => "m.created_at ASC, m.name ASC"
        }
    }

    pub fn to_select_value(&self) -> &'static str {
        match self {
            Self::NameDesc    => "name_desc",
            Self::NameAsc     => "name_asc",
            Self::UpdatedDesc => "updated_desc",
            Self::UpdatedAsc  => "updated_asc",
            Self::AddedDesc   => "added_desc",
            Self::AddedAsc    => "added_asc"
        }
    }

    pub fn from_select_value(s: &str) -> Self {
        match s {
            "name_desc"    => Self::NameDesc,
            "name_asc"     => Self::NameAsc,
            "updated_desc" => Self::UpdatedDesc,
            "updated_asc"  => Self::UpdatedAsc,
            "added_desc"   => Self::AddedDesc,
            "added_asc"    => Self::AddedAsc,
            _              => Self::NameAsc
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LibraryPage {
    pub items: Vec<MangaListItem>,
    pub has_next_page: bool
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub enabled: bool,
    pub favourited: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GlobalSearchResult {
    pub source_id: i64,
    pub source_name: String,
    pub manga: Vec<MangaListItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SearchScope {
    FavouritedOnly,
    AllEnabled,
    Sources(Vec<i64>),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ChapterFilterRow {
    pub id: i64,
    pub scanlator: Option<String>,
    pub language: String,
    pub name: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DownloadRuleKind {
    ScanlatorInclude(String),
    ScanlatorExclude(String),
    LanguageInclude(String),
    LanguageExclude(String),
    TitleContains(String),
    TitleExcludes(String),
}

impl DownloadRuleKind {
    fn matches(&self, chapter: &ChapterFilterRow) -> bool {
        match self {
            Self::ScanlatorInclude(v) | Self::ScanlatorExclude(v) =>
                chapter.scanlator.as_deref().unwrap_or("").eq_ignore_ascii_case(v),
            Self::LanguageInclude(v)  | Self::LanguageExclude(v) =>
                chapter.language.eq_ignore_ascii_case(v),
            Self::TitleContains(v)    | Self::TitleExcludes(v) =>
                chapter.name.as_deref().unwrap_or("").to_lowercase()
                    .contains(&v.to_lowercase()),
        }
    }

    pub fn is_include(&self) -> bool {
        matches!(self,
            Self::ScanlatorInclude(_) |
            Self::LanguageInclude(_)  |
            Self::TitleContains(_)
        )
    }

    pub fn axis(&self) -> u8 {
        match self {
            Self::ScanlatorInclude(_) | Self::ScanlatorExclude(_) => 0,
            Self::LanguageInclude(_)  | Self::LanguageExclude(_)  => 1,
            Self::TitleContains(_)    | Self::TitleExcludes(_)    => 2,
        }
    }

    pub fn passes(&self, chapter: &ChapterFilterRow) -> bool {
        self.matches(chapter) == self.is_include()
    }
}

impl std::fmt::Display for DownloadRuleKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadRuleKind::ScanlatorInclude(v) => write!(f, "Scanlator includes {v}"),
            DownloadRuleKind::ScanlatorExclude(v) => write!(f, "Scanlator excludes {v}"),
            DownloadRuleKind::LanguageInclude(v)  => write!(f, "Language includes {v}"),
            DownloadRuleKind::LanguageExclude(v)  => write!(f, "Language excludes {v}"),
            DownloadRuleKind::TitleContains(v)    => write!(f, "Title contains {v}"),
            DownloadRuleKind::TitleExcludes(v)    => write!(f, "Title excludes {v}"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DownloadRule {
    pub id:       i64,
    pub manga_id: i64,
    pub kind:     DownloadRuleKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ScanlatorPreference {
    pub id:        i64,
    pub manga_id:  i64,
    pub scanlator: String,
    pub priority:  i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Category {
    pub id:         i64,
    pub name:       String,
    pub sort_order: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AppSettings {
    pub flaresolverr_url:           String,
    pub library_path:               String,
    pub concurrent_page_downloads:  i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size:         i64,
    pub max_retries:                i64,
    pub initial_retry_delay_ms:     i64,
    pub max_wasm_instances:         i64,
    pub auto_scan:                  bool,
    pub scan_interval_minutes:      i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RecentUpdate {
    pub recent_updates: Vec<RecentUpdateItem>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RecentUpdateItem {
    pub manga_id: i64,
    pub manga_name: String,
    pub cover_url: Option<String>,
    pub base_url: String,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_name: Option<String>,
    pub discovered_at: std::option::Option<chrono::NaiveDateTime>,
}

#[derive(Clone, PartialEq)]
pub enum RefreshState {
    Idle,
    Running { completed: usize, total: usize },
    Done { total: usize, failed: usize },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ChapterContents {
    pub pages: Vec<Page>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Page {
    pub index: i64,
    pub url: String,
}