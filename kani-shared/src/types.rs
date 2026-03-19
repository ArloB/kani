//! Shared data types for manga information.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PreferenceDescriptor {
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub kind: PreferenceKind,
    pub group: Option<String>,
    pub requires_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
                    default_value: default_values,
                }),
            PreferenceKind::Number { min, max, step, default_value } =>
                Self::Number(wit_types::NumberInputPref { min, max, step, default_value }),
            PreferenceKind::MultiValueList { placeholder, item_label, default_values } =>
                Self::MultiValueList(wit_types::MultiValueListPref {
                    placeholder, item_label, default_value: default_values,
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
