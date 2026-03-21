//! Server-specific models (database entities and API request/response types).

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct CreateSource {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct UpdateSource {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct FetchWasmRequest {
    pub url: String,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct Settings {
    pub flaresolverr_url:           String,
    pub library_path:               std::path::PathBuf,
    pub wasm_storage_path:          std::path::PathBuf,
    pub concurrent_page_downloads:  i64,
    pub concurrent_manga_downloads: i64,
    pub chapter_queue_size:         i64,
    pub max_retries:                i64,
    pub initial_retry_delay_ms:     i64,
    pub max_wasm_instances:         i64,
    pub auto_scan:                  bool,
    pub scan_interval_minutes:      i64,
}

#[derive(Deserialize, Debug)]
pub struct SearchMangaRequest {
    pub query: String,
}

#[derive(Deserialize, Debug)]
pub struct ProxyQuery {
    pub token: String,
}

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
}

#[derive(Serialize, Deserialize, Clone, Debug, sqlx::FromRow)]
pub struct Chapter {
    pub id: i64,
    pub source_chapter_id: String,
    pub name: Option<String>,
    pub chapter_number: f64,
    pub volume: Option<i64>,
    pub language: String,
    pub scanlator: Option<String>,
    pub uploaded_at: Option<i64>,
    pub download_status: i64,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct LibraryRow {
    pub id: i64,
    pub name: String,
    pub cover_url: Option<String>,
    pub local_cover_path: Option<String>,
    pub base_url: String
}

#[derive(sqlx::FromRow)]
pub struct FilterOptionResult {
    pub id: i64,
    pub name: String,
}

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow)]
pub struct DownloadRuleRow {
    pub id:        i64,
    pub manga_id:  i64,
    pub rule_type: String,
    pub value:     String,
}

#[cfg(feature = "ssr")]
impl TryFrom<DownloadRuleRow> for crate::types::DownloadRule {
    type Error = String;

    fn try_from(row: DownloadRuleRow) -> Result<Self, Self::Error> {
        use crate::types::DownloadRuleKind;

        let kind = match row.rule_type.as_str() {
            "scanlator_include" => DownloadRuleKind::ScanlatorInclude(row.value),
            "scanlator_exclude" => DownloadRuleKind::ScanlatorExclude(row.value),
            "language_include"  => DownloadRuleKind::LanguageInclude(row.value),
            "language_exclude"  => DownloadRuleKind::LanguageExclude(row.value),
            "title_contains"    => DownloadRuleKind::TitleContains(row.value),
            "title_excludes"    => DownloadRuleKind::TitleExcludes(row.value),
            other => return Err(format!("Unknown rule_type in DB: {}", other)),
        };
        Ok(crate::types::DownloadRule { id: row.id, manga_id: row.manga_id, kind })
    }
}

mod pref_conversions {
    use crate::types::{PreferenceDescriptor, PreferenceKind, SelectOption};
    use kani_core::wasm::kani::extension::types as wit;

    impl From<wit::SelectOption> for SelectOption {
        fn from(o: wit::SelectOption) -> Self {
            Self { label: o.label, value: o.value }
        }
    }

    impl From<wit::PreferenceKind> for PreferenceKind {
        fn from(k: wit::PreferenceKind) -> Self {
            match k {
                wit::PreferenceKind::TextInput(p) => Self::TextInput {
                    placeholder: p.placeholder,
                    default_value: p.default_value,
                    is_secret: p.is_secret,
                },
                wit::PreferenceKind::Checkbox(p) => Self::Checkbox {
                    default_value: p.default_value,
                },
                wit::PreferenceKind::Select(p) => Self::Select {
                    options: p.options.into_iter().map(Into::into).collect(),
                    default_value: p.default_value,
                },
                wit::PreferenceKind::MultiSelect(p) => Self::MultiSelect {
                    options: p.options.into_iter().map(Into::into).collect(),
                    default_values: p.default_values,
                },
                wit::PreferenceKind::Number(p) => Self::Number {
                    min: p.min, max: p.max, step: p.step,
                    default_value: p.default_value,
                },
                wit::PreferenceKind::MultiValueList(p) => Self::MultiValueList {
                    placeholder: p.placeholder,
                    item_label: p.item_label,
                    default_values: p.default_values,
                },
                wit::PreferenceKind::Label(p) => Self::Label { text: p.text },
            }
        }
    }

    impl From<wit::PreferenceDescriptor> for PreferenceDescriptor {
        fn from(d: wit::PreferenceDescriptor) -> Self {
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
}