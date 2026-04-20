//! Database models and application-layer data types.

use kani_shared::types::{DownloadRule, DownloadRuleKind, NamedItem, Source};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Settings {
    pub flaresolverr_url: String,
    pub library_path: std::path::PathBuf,
    pub wasm_storage_path: std::path::PathBuf,
    pub concurrent_page_downloads: i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size: i64,
    pub max_retries: i64,
    pub initial_retry_delay_ms: i64,
    pub max_wasm_instances: i64,
    pub auto_scan: bool,
    pub scan_interval_minutes: i64,
    pub default_tracking_enabled: bool,
}

#[derive(sqlx::FromRow)]
pub struct DownloadRuleRow {
    pub id: i64,
    pub manga_id: i64,
    pub rule_type: String,
    pub value: String,
}

/// A single page entry in a downloaded chapter's CBZ archive.
#[derive(Debug, Serialize)]
pub struct PageInfo {
    pub index: usize,
    pub filename: String,
}

/// Manifest returned when opening a downloaded chapter for reading.
#[derive(Debug, Serialize)]
pub struct ChapterPageManifest {
    pub chapter_id: i64,
    pub chapter_title: String,
    pub manga_id: i64,
    pub manga_title: String,
    pub page_count: usize,
    pub pages: Vec<PageInfo>,
    pub prev_chapter_id: Option<i64>,
    pub next_chapter_id: Option<i64>,
    pub last_page_read: Option<i64>,
}

/// Full manga row as stored in the database.
#[derive(Serialize, Deserialize, Clone, Debug, sqlx::FromRow)]
pub struct Manga {
    pub id: i64,
    pub source_id: i64,
    pub source_manga_id: String,
    pub name: String,
    pub cover_url: Option<String>,
    pub local_cover_path: Option<String>,
    pub description: Option<String>,
    pub auto_download: bool,
    #[sqlx(try_from = "i64")]
    pub status: kani_shared::MangaStatus,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    pub scanlator_mode: String,
    pub download_all_preferred_only: bool,
}

/// DB row fetched when listing chapters for a manga.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ChapterRow {
    pub id: i64,
    pub source_chapter_id: String,
    pub name: Option<String>,
    pub chapter_number: f64,
    pub volume: Option<i64>,
    pub language: String,
    pub scanlator: Option<String>,
    pub uploaded_at: Option<i64>,
    pub download_status: i64,
    pub is_orphaned: bool,
    pub page_count: Option<i64>,
    pub is_read: Option<bool>,
    pub last_page_read: Option<i64>,
}

/// Slim row returned by filtered library queries (joins manga + source).
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LibraryManga {
    pub id: i64,
    pub name: String,
    pub cover_url: Option<String>,
    pub local_cover_path: Option<String>,
    pub base_url: String,
}

/// One item in the "continue reading" shelf.
#[derive(Debug, Serialize)]
pub struct ContinueReadingItem {
    pub manga_id: i64,
    pub manga_name: String,
    pub cover_url: Option<String>,
    pub local_cover_path: Option<String>,
    pub base_url: String,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub last_page: i64,
}

/// All data required to render the manga-details page.
/// URL signing and markdown rendering are left to the HTTP layer.
pub struct LocalMangaDetails {
    pub manga: Manga,
    pub source: Source,
    pub auto_scan: bool,
    pub authors: Vec<NamedItem>,
    pub artists: Vec<NamedItem>,
    pub tags: Vec<NamedItem>,
}

impl TryFrom<DownloadRuleRow> for DownloadRule {
    type Error = String;

    fn try_from(row: DownloadRuleRow) -> Result<Self, Self::Error> {
        let kind = match row.rule_type.as_str() {
            "language_include" => DownloadRuleKind::LanguageInclude(row.value),
            "language_exclude" => DownloadRuleKind::LanguageExclude(row.value),
            "title_contains" => DownloadRuleKind::TitleContains(row.value),
            "title_excludes" => DownloadRuleKind::TitleExcludes(row.value),
            "chapter_number_min" => {
                let n: f64 = row.value.parse().map_err(|_| format!("Bad f64: {}", row.value))?;
                DownloadRuleKind::ChapterNumberMin(n)
            }
            "chapter_number_max" => {
                let n: f64 = row.value.parse().map_err(|_| format!("Bad f64: {}", row.value))?;
                DownloadRuleKind::ChapterNumberMax(n)
            }
            "exclude_fractional" => DownloadRuleKind::ExcludeFractional,
            "max_age_days" => {
                let n: i32 = row.value.parse().map_err(|_| format!("Bad i32: {}", row.value))?;
                DownloadRuleKind::MaxAgeDays(n)
            }
            "published_after" => {
                let n: i64 = row.value.parse().map_err(|_| format!("Bad i64: {}", row.value))?;
                DownloadRuleKind::PublishedAfter(n)
            }
            // Scanlator rules were migrated to scanlator_preferences — skip silently.
            "scanlator_include" | "scanlator_exclude" => {
                return Err(format!("Migrated rule type skipped: {}", row.rule_type))
            }
            other => return Err(format!("Unknown rule_type in DB: {}", other)),
        };
        Ok(DownloadRule {
            id: row.id,
            manga_id: row.manga_id,
            kind,
        })
    }
}
