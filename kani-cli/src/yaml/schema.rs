use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct YamlExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base_url: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub unrestricted_http: bool,
    #[serde(default)]
    pub endpoints: Endpoints,
    #[serde(default)]
    pub filters: Vec<FilterEntry>,
    #[serde(default)]
    pub preferences: Vec<PreferenceEntry>,
    /// Optional URL template for manga canonical URL. Use `$manga_id$` as placeholder.
    pub get_url: Option<String>,
    /// Optional Mihon/Tachiyomi source ID for cross-app import matching.
    #[serde(default)]
    pub mihon_source_id: Option<i64>,
}

fn default_language() -> String {
    "en".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct Endpoints {
    pub popular: Option<PopularEndpoint>,
    pub search: Option<EndpointBody>,
    pub manga_details: Option<EndpointBody>,
    pub chapter_list: Option<EndpointBody>,
    pub pages: Option<EndpointBody>,
}

/// Popular can either delegate to another endpoint or define its own extraction.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PopularEndpoint {
    // Delegated must come first so serde tries it before Full (which is permissive).
    Delegated {
        delegate_to: String,
        #[serde(default)]
        empty_without_filters: bool,
    },
    Full(Box<EndpointBody>),
}

#[derive(Debug, Deserialize, Default)]
pub struct EndpointBody {
    pub route: Option<String>,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Query parameters. Values may contain `$var$` placeholders.
    #[serde(default)]
    pub queries: BTreeMap<String, String>,
    #[serde(default)]
    pub filter_mapping: BTreeMap<String, FilterMappingEntry>,
    #[serde(rename = "type", default)]
    pub response_type: ResponseType,
    pub container: Option<String>,
    /// Document-level variable bindings (DSL expressions), evaluated before iteration.
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
    /// Per-element field extractions (DSL expressions).
    #[serde(default)]
    pub fields: BTreeMap<String, FieldDef>,
    /// Document-level output scalars (DSL expressions), evaluated once.
    #[serde(default)]
    pub scalars: BTreeMap<String, FieldDef>,
    /// Static bool or DSL expression; overrides automatic has_next_page logic.
    pub has_next_page: Option<HasNextPage>,
    /// Optional u32 or DSL expression; populates total_pages in MangaList/ChapterList.
    pub total_pages: Option<TotalPages>,
    pub pagination: Option<PaginationCfg>,
}

fn default_method() -> String {
    "GET".to_string()
}

/// `has_next_page` can be a literal `false`/`true` or a DSL expression string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum HasNextPage {
    Static(bool),
    Expr(String),
}

/// `total_pages` can be a literal u32 or a DSL expression string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum TotalPages {
    Static(u32),
    Expr(String),
}

#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseType {
    #[default]
    Html,
    Json,
}

/// A field definition is either a bare DSL expression string or `{expr, optional}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FieldDef {
    Expr(String),
    Full {
        expr: String,
        #[serde(default)]
        optional: bool,
    },
}

impl FieldDef {
    pub fn expr_str(&self) -> &str {
        match self {
            FieldDef::Expr(e) => e,
            FieldDef::Full { expr, .. } => expr,
        }
    }

    pub fn optional(&self) -> bool {
        match self {
            FieldDef::Expr(_) => false,
            FieldDef::Full { optional, .. } => *optional,
        }
    }
}

/// Filter mapping entry: simple query-param name or a sort-pair split.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum FilterMappingEntry {
    Simple(String),
    SortPair {
        #[allow(dead_code)]
        kind: SortPairKind,
        key_template: String,
        #[serde(default)]
        direction_param: Option<String>,
    },
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortPairKind {
    SortPair,
}

// ── Pagination ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct PaginationCfg {
    pub native_page_size: usize,
    pub offset_param: String,
    pub offset_type: YamlOffsetType,
    #[serde(default = "default_page_start")]
    pub page_start: u32,
}

fn default_page_start() -> u32 {
    1
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum YamlOffsetType {
    Item,
    Page,
}

// ── Filters ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct FilterEntry {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: FilterKind,
    #[serde(default)]
    pub options: Vec<FilterOption>,
    pub default: Option<FilterDefault>,
    pub semantic: Option<FilterSemantic>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    Checkbox,
    Select,
    Sort,
    TextInput,
    Multiselect,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FilterOption {
    pub name: String,
    pub value: String,
}

/// Default value for a filter — bool for checkbox, name+value for select/sort,
/// or bare string for text_input.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum FilterDefault {
    Bool(bool),
    Option { name: String, value: String },
    Text(String),
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterSemantic {
    Author,
    Artist,
    Tag,
}

// ── Preferences ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct PreferenceEntry {
    pub key: String,
    pub label: String,
    pub kind: PreferenceKind,
    #[serde(default)]
    pub options: Vec<PrefOption>,
    #[serde(default)]
    pub default: String,
    pub description: Option<String>,
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceKind {
    Toggle,
    Select,
    Text,
    MultiValueList,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PrefOption {
    pub name: String,
    pub value: String,
}
