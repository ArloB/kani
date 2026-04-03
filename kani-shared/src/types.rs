//! Shared data types for manga information.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum PreferenceKind {
    TextInput {
        placeholder: Option<String>,
        default_value: Option<String>,
        is_secret: bool,
    },
    Checkbox {
        default_value: bool,
    },
    Select {
        options: Vec<SelectOption>,
        default_value: Option<String>,
    },
    MultiSelect {
        options: Vec<SelectOption>,
        default_values: Vec<String>,
    },
    Number {
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        default_value: Option<f64>,
    },
    MultiValueList {
        placeholder: Option<String>,
        item_label: Option<String>,
        default_values: Vec<String>,
    },
    Label {
        text: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct PreferenceDescriptor {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub kind: PreferenceKind,
    pub group: Option<String>,
    pub requires_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct PreferenceList {
    pub preferences: Vec<PreferenceDescriptor>,
}

use crate::bindings::kani::extension::types as wit_types;

impl From<SelectOption> for wit_types::SelectOption {
    fn from(o: SelectOption) -> Self {
        Self { label: o.label, value: o.value }
    }
}

impl From<PreferenceKind> for wit_types::PreferenceKind {
    fn from(k: PreferenceKind) -> Self {
        match k {
            PreferenceKind::TextInput { placeholder, default_value, is_secret } =>
                Self::TextInput(wit_types::TextInputPref { placeholder, default_value, is_secret }),
            PreferenceKind::Checkbox { default_value } =>
                Self::Checkbox(wit_types::CheckboxPref { default_value }),
            PreferenceKind::Select { options, default_value } =>
                Self::Select(wit_types::SelectPref {
                    options: options.into_iter().map(Into::into).collect(),
                    default_value,
                }),
            PreferenceKind::MultiSelect { options, default_values } =>
                Self::MultiSelect(wit_types::MultiSelectPref {
                    options: options.into_iter().map(Into::into).collect(),
                    default_values,
                }),
            PreferenceKind::Number { min, max, step, default_value } =>
                Self::Number(wit_types::NumberInputPref { min, max, step, default_value }),
            PreferenceKind::MultiValueList { placeholder, item_label, default_values } =>
                Self::MultiValueList(wit_types::MultiValueListPref {
                    placeholder, item_label, default_values,
                }),
            PreferenceKind::Label { text } =>
                Self::Label(wit_types::LabelPref { text }),
        }
    }
}

impl From<PreferenceDescriptor> for wit_types::PreferenceDescriptor {
    fn from(d: PreferenceDescriptor) -> Self {
        Self {
            key: d.key,
            title: d.title,
            description: d.description,
            kind: d.kind.into(),
            group: d.group,
            requires_key: d.requires_key,
        }
    }
}

impl From<wit_types::SelectOption> for SelectOption {
    fn from(o: wit_types::SelectOption) -> Self {
        Self { label: o.label, value: o.value }
    }
}

impl From<wit_types::PreferenceKind> for PreferenceKind {
    fn from(k: wit_types::PreferenceKind) -> Self {
        match k {
            wit_types::PreferenceKind::TextInput(p) => Self::TextInput {
                placeholder: p.placeholder,
                default_value: p.default_value,
                is_secret: p.is_secret,
            },
            wit_types::PreferenceKind::Checkbox(p) => Self::Checkbox {
                default_value: p.default_value,
            },
            wit_types::PreferenceKind::Select(p) => Self::Select {
                options: p.options.into_iter().map(Into::into).collect(),
                default_value: p.default_value,
            },
            wit_types::PreferenceKind::MultiSelect(p) => Self::MultiSelect {
                options: p.options.into_iter().map(Into::into).collect(),
                default_values: p.default_values,
            },
            wit_types::PreferenceKind::Number(p) => Self::Number {
                min: p.min, max: p.max, step: p.step,
                default_value: p.default_value,
            },
            wit_types::PreferenceKind::MultiValueList(p) => Self::MultiValueList {
                placeholder: p.placeholder,
                item_label: p.item_label,
                default_values: p.default_values,
            },
            wit_types::PreferenceKind::Label(p) => Self::Label { text: p.text },
        }
    }
}

impl From<wit_types::PreferenceDescriptor> for PreferenceDescriptor {
    fn from(d: wit_types::PreferenceDescriptor) -> Self {
        Self {
            key: d.key,
            title: d.title,
            description: d.description,
            kind: d.kind.into(),
            group: d.group,
            requires_key: d.requires_key,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct FilterList {
    pub filters: Vec<Filter>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct Filter {
    pub name: String,
    pub filter_type: FilterType,
    pub options: Vec<FilterOption>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum FilterType {
    Select,
    Checkbox,
    TextInput,
    Sort,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct FilterOption {
    pub name: String,
    pub value: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

    ChapterDeferred {
        chapter_id: i64,
        chapter_name: String,
        reason: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaListItem {
    pub id: String,
    pub title: String,
    pub cover_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MangaList {
    pub manga: Vec<MangaListItem>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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
    #[serde(default)]
    pub is_orphaned: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ChapterList {
    pub chapters: Vec<Chapter>,
    pub has_next_page: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct LibraryPage {
    pub items: Vec<MangaListItem>,
    pub has_next_page: bool
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub enabled: bool,
    pub favourited: bool,
    pub unrestricted_http: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct GlobalSearchResult {
    pub source_id: i64,
    pub source_name: String,
    pub has_next_page: bool,
    pub manga: Vec<MangaListItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum SearchScope {
    FavouritedOnly,
    AllEnabled,
    Sources(Vec<i64>),
}

#[derive(Debug, Clone, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ChapterFilterRow {
    pub id: i64,
    pub scanlator: Option<String>,
    pub language: String,
    pub name: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct DownloadRule {
    pub id:       i64,
    pub manga_id: i64,
    pub kind:     DownloadRuleKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct ScanlatorPreference {
    pub id:        i64,
    pub manga_id:  i64,
    pub scanlator: String,
    pub priority:  i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct Category {
    pub id:         i64,
    pub name:       String,
    pub sort_order: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct RecentUpdate {
    pub recent_updates: Vec<RecentUpdateItem>,
    pub has_next_page: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct RecentUpdateItem {
    pub manga_id: i64,
    pub manga_name: String,
    pub cover_url: Option<String>,
    #[serde(skip)]
    pub local_cover_path: Option<String>,
    pub base_url: String,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_name: Option<String>,
    #[ts(type = "string")]
    pub discovered_at: std::option::Option<time::OffsetDateTime>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ChapterContents {
    pub pages: Vec<Page>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct Page {
    pub index: i64,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MigrationResult {
    pub chapters_matched: usize,
    pub chapters_orphaned: usize,
    pub chapters_new: usize,
    pub chapters_kept: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct MigrationPreview {
    pub target_title: String,
    pub target_cover_url: Option<String>,
    pub chapters_matched: usize,
    pub chapters_orphaned: usize,
    pub chapters_new: usize,
    pub downloaded_chapters_at_risk: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct AuthenticatedUser {
    pub id:       i64,
    pub username: String,
    pub email:    String,
    pub roles:    Vec<String>,
}

impl AuthenticatedUser {
    pub fn has_role(&self, slug: &str) -> bool {
        self.roles.iter().any(|r| r == slug)
    }
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct DownloadSettings {
    pub concurrent_page_downloads:  i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size:         i64,
    pub max_retries:                i64,
    pub initial_retry_delay_ms:     i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct ScanSettings {
    pub auto_scan:             bool,
    pub scan_interval_minutes: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub struct AdvancedSettings {
    pub flaresolverr_url:  String,
    pub library_path:      String,
    pub max_wasm_instances: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, ts_rs::TS)]
#[ts(export, export_to = "bindings/")]
pub enum SettingsUpdate {
    Download(DownloadSettings),
    Scan(ScanSettings),
    Advanced(AdvancedSettings),
}
