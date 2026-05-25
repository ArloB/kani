use crate::yaml::schema::{
    FilterEntry, FilterMappingEntry, PaginationCfg, PreferenceEntry, ResponseType,
};
use kani_shared::ast::Expr;

/// A fully-validated extension with all DSL strings compiled into `Expr` trees.
pub struct ValidatedExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub language: String,
    pub nsfw: bool,
    pub unrestricted_http: bool,
    pub popular: Option<ValidatedPopular>,
    pub search: Option<ValidatedEndpoint>,
    pub manga_details: Option<ValidatedEndpoint>,
    pub chapter_list: Option<ValidatedEndpoint>,
    pub pages: Option<ValidatedEndpoint>,
    pub filters: Vec<FilterEntry>,
    pub preferences: Vec<PreferenceEntry>,
    /// Optional URL template, e.g. `"https://example.com/manga/$manga_id$"`.
    pub get_url: Option<String>,
    /// Optional Mihon/Tachiyomi source ID for cross-app import matching.
    pub mihon_source_id: Option<i64>,
}

pub enum ValidatedPopular {
    Delegated {
        delegate_to: String,
        empty_without_filters: bool,
    },
    Full(Box<ValidatedEndpoint>),
}

pub struct ValidatedEndpoint {
    pub route: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub queries: Vec<QueryEntry>,
    pub filter_mapping: Vec<(String, FilterMappingEntry)>,
    pub response_type: ResponseType,
    pub container: String,
    pub bindings: Vec<ValidatedBinding>,
    pub fields: Vec<ValidatedField>,
    pub scalars: Vec<ValidatedField>,
    pub has_next_page: ValidatedHnp,
    pub total_pages: ValidatedTotalPages,
    pub pagination: Option<PaginationCfg>,
}

pub enum ValidatedHnp {
    Static(bool),
    Scalar(Expr),
    /// Default: true (caller checks if a `has_next_page` scalar was added separately).
    Default,
}

pub enum ValidatedTotalPages {
    Static(u32),
    Scalar(Expr),
    None,
}

pub struct QueryEntry {
    pub key: String,
    pub value: QueryValue,
}

pub enum QueryValue {
    /// Literal string appended verbatim.
    Static(String),
    /// Single `$var$` placeholder → Rust function-arg identifier.
    Arg(String),
}

pub struct ValidatedBinding {
    /// Variable name without the `$` prefix.
    pub name: String,
    pub expr: Expr,
}

pub struct ValidatedField {
    pub name: String,
    pub source: FieldSource,
    pub optional: bool,
}

pub enum FieldSource {
    /// Expression to include as a blueprint field; evaluated against the document.
    Blueprint(Expr),
    /// Value taken directly from the named Rust function argument (e.g. `manga_id`).
    FnArg(String),
}
