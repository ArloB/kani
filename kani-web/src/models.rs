//! Server-specific models (database entities and API request/response types).

use kani_app::ids::{ChapterId, MangaId};
use serde::Deserialize;

fn validate_https_url(value: &str, _: &()) -> garde::Result {
    if value.starts_with("https://") {
        Ok(())
    } else {
        Err(garde::Error::new("URL must use HTTPS scheme"))
    }
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct CreateSource {
    #[garde(length(min = 1, max = 100))]
    pub name: String,
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct UpdateSource {
    #[garde(inner(length(min = 1, max = 100)))]
    pub name: Option<String>,
    #[garde(inner(length(min = 1, max = 50)))]
    pub version: Option<String>,
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct FetchWasmRequest {
    #[garde(length(min = 1, max = 2048), custom(validate_https_url))]
    pub url: String,
}

// Types moved to kani-app. Re-exported here so that existing kani-web code
// (rest.rs, tests) continues to compile unchanged.
pub use kani_app::models::{DownloadRuleRow, LibraryManga, Manga, Settings};

#[derive(serde::Deserialize, Default, Debug, utoipa::ToSchema)]
pub struct RefreshMangaRequest {
    pub fields: Option<Vec<String>>,
    pub fetch_chapters: Option<bool>,
    pub clear_overrides: Option<bool>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct UpdateLocalMetadataRequest {
    pub local_name: Option<String>,
    pub local_description: Option<String>,
    pub local_status: Option<i64>,
    pub authors: Option<Vec<String>>,
    pub artists: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct SearchMangaRequest {
    #[garde(inner(length(max = 200)))]
    pub query: Option<String>,
    #[garde(skip)]
    pub filters: Option<String>,
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct PopularMangaQuery {
    #[garde(skip)]
    pub filters: Option<String>,
}

#[derive(garde::Validate, Deserialize, Debug, utoipa::ToSchema)]
pub struct ProxyQuery {
    #[garde(length(min = 1, max = 4096))]
    pub token: String,
    #[garde(skip)]
    pub transform: Option<String>,
    #[garde(skip)]
    pub w: Option<u32>,
    #[garde(skip)]
    pub format: Option<String>,
    #[garde(skip)]
    pub q: Option<u8>,
}

#[derive(sqlx::FromRow)]
pub struct FilterOptionResult {
    pub id: i64,
    pub name: String,
}

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct LibraryQuery {
    #[garde(range(min = 1))]
    pub page: i32,
    #[garde(range(min = 1, max = 200))]
    pub page_size: i32,
    #[garde(skip)]
    pub search: Option<String>,
    #[garde(skip)]
    pub status_filter: Option<i64>,
    #[garde(skip)]
    pub tag_filter: Option<i64>,
    #[garde(skip)]
    pub author_filter: Option<i64>,
    #[garde(skip)]
    pub artist_filter: Option<i64>,
    #[garde(skip)]
    pub category_filter: Option<i64>,
    #[garde(skip)]
    pub reading_status_filter: Option<i64>,
    #[garde(skip)]
    #[serde(default)]
    pub hide_no_unread: bool,
    #[garde(skip)]
    #[serde(default)]
    pub hide_completed_status: bool,
    #[garde(skip)]
    pub source_id: Option<i64>,
    #[garde(skip)]
    pub collection_id: Option<i64>,
    #[garde(skip)]
    #[serde(default)]
    #[schema(value_type = String)]
    pub sort_by: kani_shared::MangaSortOrder,
}

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct LocalChaptersQuery {
    #[garde(range(min = 1))]
    pub page: i32,
    #[garde(range(min = 1, max = 200))]
    #[serde(default = "default_chapter_page_size")]
    pub page_size: i32,
    #[garde(skip)]
    #[serde(default)]
    #[schema(value_type = String)]
    pub sort_order: kani_shared::ChapterSortOrder,
    /// `true` = downloaded only, `false` = undownloaded only, absent = all
    #[garde(skip)]
    pub filter_downloaded: Option<bool>,
    /// `true` = unread only
    #[garde(skip)]
    pub filter_unread: Option<bool>,
    /// Limit to a specific scanlator when set
    #[garde(skip)]
    pub filter_scanlator: Option<String>,
}

/// Query parameters for the chapter-IDs endpoint (no pagination).
#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ChapterIdsQuery {
    #[garde(skip)]
    #[serde(default)]
    #[schema(value_type = String)]
    pub sort_order: kani_shared::ChapterSortOrder,
    /// `true` = downloaded only, `false` = undownloaded only, absent = all
    #[garde(skip)]
    pub filter_downloaded: Option<bool>,
    /// `true` = unread only
    #[garde(skip)]
    pub filter_unread: Option<bool>,
    /// Limit to a specific scanlator when set
    #[garde(skip)]
    pub filter_scanlator: Option<String>,
    /// When true, applies scanlator preferences + download rules to return
    /// only the preferred one version per chapter number (undownloaded only).
    #[garde(skip)]
    #[serde(default)]
    pub preferred_only: bool,
}

fn default_chapter_page_size() -> i32 {
    50
}

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PageQuery {
    #[garde(range(min = 1))]
    pub page: i32,
}

fn deserialize_search_scope<'de, D>(deserializer: D) -> Result<kani_shared::SearchScope, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use kani_shared::SearchScope;
    use serde::Deserialize;
    let s = String::deserialize(deserializer)?;
    // Try to parse as JSON first (handles {"Sources":[1,2]})
    if let Ok(scope) = serde_json::from_str::<SearchScope>(&s) {
        return Ok(scope);
    }
    // Fall back to unit variant name (handles FavouritedOnly, AllEnabled)
    serde_json::from_str::<SearchScope>(&format!("\"{}\"", s)).map_err(serde::de::Error::custom)
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct GlobalSearchQuery {
    pub query: String,
    #[serde(deserialize_with = "deserialize_search_scope")]
    #[schema(value_type = Object)]
    pub scope: kani_shared::SearchScope,
    pub page: i32,
    pub page_size: i32,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AddDownloadRuleRequest {
    #[schema(value_type = Object)]
    pub kind: kani_shared::DownloadRuleKind,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct UpdateDownloadRuleRequest {
    #[schema(value_type = Object)]
    pub kind: kani_shared::DownloadRuleKind,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ReorderDownloadRulesRequest {
    pub ordered_ids: Vec<i64>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PreviewDownloadRulesRequest {
    #[schema(value_type = Vec<Object>)]
    pub kinds: Vec<kani_shared::DownloadRuleKind>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetScanlatorPrefRequest {
    pub scanlator: String,
    pub priority: i64,
    #[serde(default)]
    pub blocked: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetScanlatorModeRequest {
    pub mode: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    pub sort_order: i64,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct RenameCategoryRequest {
    pub name: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ReorderCategoriesRequest {
    pub ordered_ids: Vec<i64>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetMangaCategoriesRequest {
    pub category_ids: Vec<i64>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetPreferenceRequest {
    pub value: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ListItemRequest {
    pub item: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ToggleSelectRequest {
    pub item: String,
    pub selected: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ToggleEnabledRequest {
    pub enabled: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ToggleFavouritedRequest {
    pub favourited: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ToggleAutoDownloadRequest {
    pub enabled: bool,
}

/// Body for `POST /manga/scan`. Either scan all manga or a specific list.
#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
#[serde(untagged)]
pub enum ScanMangaRequest {
    /// Scan specific manga by ID.
    Ids {
        #[schema(value_type = Vec<i64>)]
        ids: Vec<MangaId>,
    },
    /// Scan all manga in the library. Send `{ "ids": "all" }`.
    All { ids: ScanAll },
}

/// Sentinel value — used in `ScanMangaRequest::All`.
#[derive(Debug, utoipa::ToSchema)]
pub struct ScanAll;

impl<'de> serde::Deserialize<'de> for ScanAll {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s == "all" {
            Ok(ScanAll)
        } else {
            Err(serde::de::Error::custom(format!(
                "expected \"all\", got {s:?}"
            )))
        }
    }
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PreviewMigrationRequest {
    pub target_source_id: i64,
    pub target_source_manga_id: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct MigrateMangaRequest {
    pub target_source_id: i64,
    pub target_source_manga_id: String,
    pub keep_orphaned_downloads: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetChapterProgressRequest {
    pub page: i64,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetReadStatusRequest {
    #[schema(value_type = Vec<i64>)]
    pub chapter_ids: Vec<ChapterId>,
    pub is_read: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetMangaTrackingRequest {
    #[schema(value_type = Option<String>)]
    pub status: Option<kani_shared::types::MangaTrackingStatus>,
    pub score: Option<f64>,
    pub tracking_enabled: Option<bool>,
    pub notify_new_chapters: Option<bool>,
    pub reading_direction: Option<String>,
    pub reader_prefs: Option<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ToggleBookmarkRequest {
    pub page_index: i64,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetChapterNoteRequest {
    pub note: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PaceQuery {
    pub period: Option<i32>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct TrackerAuthUrlQuery {
    pub redirect_uri: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct TrackerCallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetTrackerConfigRequest {
    pub client_id: String,
    /// Omit to keep existing secret; set to empty string to clear.
    pub client_secret: Option<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct TrackerSearchQuery {
    pub query: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SetTrackerMappingRequest {
    pub tracker_id: i64,
    pub tracker_manga_id: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct MarkUpToRequest {
    pub chapter_number: f64,
    pub is_read: bool,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct ContinueReadingShelfQuery {
    #[serde(default = "default_shelf_limit")]
    pub limit: i64,
}

fn default_shelf_limit() -> i64 {
    12
}

// ── Admin / user-management request types ─────────────────────────────────────

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AdminCreateUserRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    /// Roles to assign in addition to the default "user" role.
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AdminUpdateUserRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub password: Option<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AdminGrantRoleRequest {
    pub role_slug: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AdminCreateRoleRequest {
    pub slug: String,
    pub parent: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AdminUpdateRoleRequest {
    pub description: Option<String>,
    pub permissions: Option<Vec<String>>,
}

// ── Admin logs queries ────────────────────────────────────────────────────────

#[derive(garde::Validate, serde::Deserialize, Debug, Default, utoipa::ToSchema)]
pub struct LogsQuery {
    /// Comma-separated levels, e.g. "error,warn". Empty = all levels.
    #[garde(skip)]
    pub level: Option<String>,
    /// Comma-separated source tags, e.g. "app,http". Empty = all sources.
    #[garde(skip)]
    pub source: Option<String>,
    #[garde(skip)]
    pub from: Option<String>,
    #[garde(skip)]
    pub to: Option<String>,
    #[garde(skip)]
    pub search: Option<String>,
    #[garde(range(min = 1))]
    pub page: Option<i32>,
    #[garde(range(min = 1, max = 500))]
    pub page_size: Option<i32>,
    /// "json" | "text" — only used by the download endpoint.
    #[garde(skip)]
    pub format: Option<String>,
}

#[derive(garde::Validate, serde::Deserialize, Debug, Default, utoipa::ToSchema)]
pub struct AuditLogQuery {
    #[garde(skip)]
    pub user_id: Option<i64>,
    #[garde(skip)]
    pub action: Option<String>,
    #[garde(skip)]
    pub from: Option<String>,
    #[garde(skip)]
    pub to: Option<String>,
    #[garde(skip)]
    pub search: Option<String>,
    #[garde(range(min = 1))]
    pub page: Option<i32>,
    #[garde(range(min = 1, max = 200))]
    pub page_size: Option<i32>,
    /// "json" | "csv" — only used by the download endpoint.
    #[garde(skip)]
    pub format: Option<String>,
}

// ── Password reset / email verification request types ─────────────────────────

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PasswordResetRequestBody {
    pub email: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PasswordResetConfirmBody {
    pub token: String,
    pub new_password: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct TokenQuery {
    pub token: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct SendTestEmailBody {
    pub to: String,
}

// ── Reading stats query ───────────────────────────────────────────────────────

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct StatsQuery {
    /// Number of days for the daily_activity window. Default 90.
    #[garde(range(min = 1, max = 365))]
    pub period: Option<i32>,
    /// Reserved: comma-separated list of stat blocks to compute.
    #[garde(skip)]
    pub metrics: Option<String>,
}

// ── Filesystem browser ────────────────────────────────────────────────────────

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct FsBrowseQuery {
    #[garde(length(min = 1, max = 4096))]
    pub path: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct FsMkdirBody {
    pub path: String,
    pub name: String,
}

#[derive(serde::Serialize, Debug, utoipa::ToSchema)]
pub struct FsBrowseResponse {
    pub path: String,
    pub segments: Vec<String>,
    pub dirs: Vec<String>,
    pub drives: Vec<String>,
}

#[derive(serde::Serialize, Debug, utoipa::ToSchema)]
pub struct FsMkdirResponse {
    pub path: String,
}

// ── Path migration ────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct PathMigrateBody {
    pub field: String,
    pub new_path: String,
}

#[derive(serde::Serialize, Debug, utoipa::ToSchema)]
pub struct PathMigrateEstimateResponse {
    pub current_bytes: u64,
    pub available_bytes: u64,
    pub can_migrate: bool,
    pub reason: Option<String>,
}

// ── Repository management ─────────────────────────────────────────────────────

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct AddRepoRequest {
    #[garde(length(min = 1, max = 2048), custom(validate_https_url))]
    pub url: String,
    #[garde(skip)]
    pub confirm_fingerprint: Option<String>,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct InstallFromRepoRequest {
    pub repo_id: i64,
    pub extension_id: String,
}

#[derive(serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct UpdateFromRepoRequest {
    pub repo_id: i64,
    pub extension_id: String,
}

#[derive(garde::Validate, serde::Deserialize, Debug, utoipa::ToSchema)]
pub struct BlockRepoRequest {
    #[garde(length(min = 1, max = 2048), custom(validate_https_url))]
    pub url: String,
    #[garde(length(min = 1, max = 500))]
    pub reason: String,
}
