#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Expr {
    /// The current iteration element (inside a container loop)
    SelfRef,

    /// Select from the document root: dom("selector")
    Dom(String),

    /// Navigate to a JSON value using a JSON Pointer
    Json(String),

    /// Reference a bound variable: $name
    Var(String),

    /// A string literal
    Literal(String),

    /// A numeric literal
    Number(f64),

    /// Null value
    Null,

    Bool(bool),

    BinaryOperation {
        op: Op,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// .attr("name") - get attribute value
    Attr {
        target: Box<Expr>,
        name: String,
    },

    /// .text() - get text content
    Text {
        target: Box<Expr>,
    },

    /// .inner_html() - get inner HTML
    InnerHtml {
        target: Box<Expr>,
    },

    /// .select("selector") - sub-select within an element
    Select {
        target: Box<Expr>,
        selector: String,
    },

    /// .first("selector") - first matching child
    First {
        target: Box<Expr>,
        selector: String,
    },

    /// .split("delim") - split string, returning a List<String>
    Split {
        target: Box<Expr>,
        delimiter: String,
    },

    /// .at(n) - get element at index from a List; negative indices count from the end
    At {
        target: Box<Expr>,
        index: i32,
    },

    /// .replace("from", "to")
    Replace {
        target: Box<Expr>,
        from: String,
        to: String,
    },

    /// .trim()
    Trim {
        target: Box<Expr>,
    },

    /// .prepend(expr) - prepend another expression's result
    Prepend {
        target: Box<Expr>,
        prefix: Box<Expr>,
    },

    /// .append(expr) - append another expression's result
    Append {
        target: Box<Expr>,
        suffix: Box<Expr>,
    },

    /// .to_lowercase()
    Lower {
        target: Box<Expr>,
    },

    /// .matches("regex") - test if value matches pattern
    Matches {
        target: Box<Expr>,
        pattern: String,
    },

    /// .capture("regex") - returns all capture groups from the first match as List<Str|Null>,
    /// where index 0 is the whole match and index n is capture group n.
    /// Returns an empty list if there is no match.
    Capture {
        target: Box<Expr>,
        pattern: String,
    },

    /// .parse_float() - parse string to f64
    ParseFloat {
        target: Box<Expr>,
    },

    /// .parse_int() - parse string to i64
    ParseInt {
        target: Box<Expr>,
    },

    /// .ptr("/path/to/value") - JSON Pointer navigation
    JsonPtr {
        target: Box<Expr>,
        pointer: String,
    },

    /// .str() - Extract as a string.
    JsonStr {
        target: Box<Expr>,
    },

    /// .int() - Extract as an integer.
    JsonInt {
        target: Box<Expr>,
    },

    /// .float() - Extract as a float.
    JsonFloat {
        target: Box<Expr>,
    },

    /// .bool() - Extract as a boolean.
    JsonBool {
        target: Box<Expr>,
    },

    /// .array_len() - length of JSON array at pointer
    ArrayLen {
        target: Box<Expr>,
    },

    /// .keys() - get the keys of a JSON object as a List of strings
    JsonKeys {
        target: Box<Expr>,
    },

    /// .has_class("name") - test if element has a CSS class
    HasClass {
        target: Box<Expr>,
        class: String,
    },

    /// .children() - direct child elements as a List<Element>
    Children {
        target: Box<Expr>,
    },

    /// .starts_with("prefix") - test if string starts with prefix
    StartsWith {
        target: Box<Expr>,
        prefix: String,
    },

    /// .ends_with("suffix") - test if string ends with suffix
    EndsWith {
        target: Box<Expr>,
        suffix: String,
    },

    /// .slice(start, end) - substring by character index (0-based, exclusive end; negatives count from end)
    Slice {
        target: Box<Expr>,
        start: i32,
        end: Option<i32>,
    },

    /// let $name = expr; continuation
    Let {
        name: String,
        value: Box<Expr>,
        body: Box<Expr>,
    },

    /// expr.fallback(default) - use default if expr is null/empty/failed
    Fallback {
        target: Box<Expr>,
        default: Box<Expr>,
    },

    /// expr.map_status({ "publishing": "ongoing", ... }) - lookup table
    Lookup {
        target: Box<Expr>,
        table: Vec<(String, String)>,
    },

    /// Iterate over a give list applying the transform to each element, producing a List
    Map {
        target: Box<Expr>,
        transform: Box<Expr>,
    },

    /// Same as map, but results are flattened into a single List result
    FlatMap {
        target: Box<Expr>,
        transform: Box<Expr>,
    },

    /// Concatenate multiple strings
    Concat(Vec<Expr>),

    /// Perform a left-fold applying transform on a list, starting with base as the base case
    Fold {
        target: Box<Expr>,
        transform: Box<Expr>,
        base: Box<Expr>,
    },

    /// Filter elements from a list based on a filter predicate
    Filter {
        target: Box<Expr>,
        filter: Box<Expr>,
    },

    /// Evaluate multiple expressions and return them as a list/array
    List(Vec<Expr>),

    /// Parse a date string with a format pattern, returning unix timestamp
    DateParse {
        target: Box<Expr>,
        format: String,
    },

    /// Parse an RFC3339 date string, returning unix timestamp
    DateParseRfc3339 {
        target: Box<Expr>,
    },

    /// Resolve a relative URL against a base
    ResolveUrl {
        target: Box<Expr>,
        base: Box<Expr>,
    },

    /// The iteration index (0-based) inside a container loop
    Index,

    /// if condition then value_if_true else value_if_false
    If {
        condition: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
    },

    /// .to_string() — convert Int, Float, Bool to their string representation; Null stays Null
    ToString {
        target: Box<Expr>,
    },

    /// .join("delim") — join a List<String> into a single string with a delimiter; Null items skipped
    Join {
        target: Box<Expr>,
        delimiter: String,
    },

    /// .get(key_expr) — dynamic JSON object field access by an expression-evaluated key
    JsonGet {
        target: Box<Expr>,
        key: Box<Expr>,
    },

    /// .find(key, value) — find the first element in a JSON array where obj[key] == value
    JsonFind {
        target: Box<Expr>,
        key: Box<Expr>,
        value: Box<Expr>,
    },

    /// json_array([a, b, ...]) — construct a JSON array from N evaluated expressions
    JsonArray(Vec<Expr>),

    /// .json_fold() — reduce all elements of a JSON array via merge
    /// e.g. [{"en":"A"},{"ja":"B"}] → {"en":"A","ja":"B"}; [[1,2],[3,4]] → [1,2,3,4]
    JsonFold {
        target: Box<Expr>,
    },

    /// merge([list1, list2, ...]) — concatenate multiple lists into one
    Merge(Vec<Expr>),

    /// pref("key") — read an extension preference value; returns String or Null if unset
    Pref(String),

    /// format("template {}", arg1, arg2, ...) — interpolate `{}` placeholders with evaluated args
    Format {
        template: String,
        args: Vec<Expr>,
    },

    /// .not() — boolean negation; Null is treated as false (not(null) = true)
    Not {
        target: Box<Expr>,
    },

    /// .string_len() — number of Unicode characters in the string; returns Int
    StringLen {
        target: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

/// How the offset/page query parameter is calculated for each chunk fetch.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub enum OffsetType {
    /// Param value = absolute item count: 0, 32, 64, …
    ItemOffset,
    /// Param value = page number starting at `start` (typically 0 or 1)
    PageNumber { start: u32 },
}

/// Declares that a source paginates in fixed-size chunks, so the framework can
/// handle the offset algebra instead of each extension doing it manually.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct PaginationConfig {
    /// How many items the source actually returns per chunk (its real page size)
    pub native_page_size: usize,
    /// Query parameter name the source uses for the offset/page (e.g. "offset", "page")
    pub offset_param: String,
    pub offset_type: OffsetType,
}

/// A complete extraction blueprint sent across the FFI boundary.
#[derive(Debug, Clone)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Blueprint {
    /// HTTP Request
    pub request: Option<RequestDef>,

    /// CSS selector (HTML) or JSON Pointer (JSON) for the repeating container
    pub container: String,

    /// Fields to extract from each container element
    pub fields: Vec<FieldDef>,

    /// Variables bound before iteration (e.g., from document-level selectors)
    pub bindings: Vec<Binding>,

    /// Document-level fields evaluated once (not per-element); returned in output alongside rows
    pub scalars: Vec<FieldDef>,

    /// When set, `paginated-extract-html` handles multi-chunk fetching automatically
    pub pagination: Option<PaginationConfig>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct FieldDef {
    /// The output field name (e.g., "id", "title", "cover_url")
    pub name: String,

    /// The extraction expression tree
    pub expr: Expr,

    /// Whether this field is optional (null allowed in output)
    pub optional: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct Binding {
    pub name: String,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    any(feature = "host", feature = "builder"),
    derive(serde::Serialize, serde::Deserialize)
)]
pub struct RequestDef {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub queries: Vec<(String, String)>,
}

// ============================================================
// Builder API — enabled by the `builder` feature flag.
// All methods are #[inline] so they compile to zero overhead.
// ============================================================

#[cfg(feature = "builder")]
#[allow(clippy::should_implement_trait)]
impl Expr {
    // ── Leaf constructors ────────────────────────────────────────────────────
    #[inline]
    pub fn self_ref() -> Self {
        Expr::SelfRef
    }
    #[inline]
    pub fn index() -> Self {
        Expr::Index
    }
    #[inline]
    pub fn null() -> Self {
        Expr::Null
    }
    #[inline]
    pub fn true_val() -> Self {
        Expr::Bool(true)
    }
    #[inline]
    pub fn false_val() -> Self {
        Expr::Bool(false)
    }
    #[inline]
    pub fn lit(s: impl Into<String>) -> Self {
        Expr::Literal(s.into())
    }
    #[inline]
    pub fn num(n: f64) -> Self {
        Expr::Number(n)
    }
    #[inline]
    pub fn var(name: impl Into<String>) -> Self {
        Expr::Var(name.into())
    }
    #[inline]
    pub fn bool(bool: bool) -> Self {
        Expr::Bool(bool)
    }
    #[inline]
    pub fn dom(selector: impl Into<String>) -> Self {
        Expr::Dom(selector.into())
    }
    #[inline]
    pub fn json_root(pointer: impl Into<String>) -> Self {
        Expr::Json(pointer.into())
    }

    // ── HTML methods ─────────────────────────────────────────────────────────
    #[inline]
    pub fn attr(self, name: impl Into<String>) -> Self {
        Expr::Attr {
            target: Box::new(self),
            name: name.into(),
        }
    }
    #[inline]
    pub fn text(self) -> Self {
        Expr::Text {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn inner_html(self) -> Self {
        Expr::InnerHtml {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn select(self, selector: impl Into<String>) -> Self {
        Expr::Select {
            target: Box::new(self),
            selector: selector.into(),
        }
    }
    #[inline]
    pub fn first(self, selector: impl Into<String>) -> Self {
        Expr::First {
            target: Box::new(self),
            selector: selector.into(),
        }
    }
    #[inline]
    pub fn has_class(self, class: impl Into<String>) -> Self {
        Expr::HasClass {
            target: Box::new(self),
            class: class.into(),
        }
    }
    #[inline]
    pub fn children(self) -> Self {
        Expr::Children {
            target: Box::new(self),
        }
    }

    // ── String methods ───────────────────────────────────────────────────────
    #[inline]
    pub fn split(self, delimiter: impl Into<String>) -> Self {
        Expr::Split {
            target: Box::new(self),
            delimiter: delimiter.into(),
        }
    }
    #[inline]
    pub fn at(self, index: i32) -> Self {
        Expr::At {
            target: Box::new(self),
            index,
        }
    }
    #[inline]
    pub fn trim(self) -> Self {
        Expr::Trim {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn lower(self) -> Self {
        Expr::Lower {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn replace(self, from: impl Into<String>, to: impl Into<String>) -> Self {
        Expr::Replace {
            target: Box::new(self),
            from: from.into(),
            to: to.into(),
        }
    }
    #[inline]
    pub fn slice(self, start: i32, end: Option<i32>) -> Self {
        Expr::Slice {
            target: Box::new(self),
            start,
            end,
        }
    }
    #[inline]
    pub fn starts_with(self, prefix: impl Into<String>) -> Self {
        Expr::StartsWith {
            target: Box::new(self),
            prefix: prefix.into(),
        }
    }
    #[inline]
    pub fn ends_with(self, suffix: impl Into<String>) -> Self {
        Expr::EndsWith {
            target: Box::new(self),
            suffix: suffix.into(),
        }
    }
    #[inline]
    pub fn matches(self, pattern: impl Into<String>) -> Self {
        Expr::Matches {
            target: Box::new(self),
            pattern: pattern.into(),
        }
    }
    #[inline]
    pub fn capture(self, pattern: impl Into<String>) -> Self {
        Expr::Capture {
            target: Box::new(self),
            pattern: pattern.into(),
        }
    }
    #[inline]
    pub fn append(self, suffix: Expr) -> Self {
        Expr::Append {
            target: Box::new(self),
            suffix: Box::new(suffix),
        }
    }
    #[inline]
    pub fn append_str(self, s: impl Into<String>) -> Self {
        self.append(Expr::Literal(s.into()))
    }
    #[inline]
    pub fn prepend(self, prefix: Expr) -> Self {
        Expr::Prepend {
            target: Box::new(self),
            prefix: Box::new(prefix),
        }
    }
    #[inline]
    pub fn prepend_str(self, s: impl Into<String>) -> Self {
        self.prepend(Expr::Literal(s.into()))
    }

    // ── Parse / type conversion ──────────────────────────────────────────────
    #[inline]
    pub fn parse_float(self) -> Self {
        Expr::ParseFloat {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn parse_int(self) -> Self {
        Expr::ParseInt {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn stringify(self) -> Self {
        Expr::ToString {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn date_parse(self, format: impl Into<String>) -> Self {
        Expr::DateParse {
            target: Box::new(self),
            format: format.into(),
        }
    }
    #[inline]
    pub fn date_parse_rfc3339(self) -> Self {
        Expr::DateParseRfc3339 {
            target: Box::new(self),
        }
    }

    // ── Control flow ─────────────────────────────────────────────────────────
    #[inline]
    pub fn fallback(self, default: Expr) -> Self {
        Expr::Fallback {
            target: Box::new(self),
            default: Box::new(default),
        }
    }
    #[inline]
    pub fn fallback_str(self, default: impl Into<String>) -> Self {
        self.fallback(Expr::Literal(default.into()))
    }
    #[inline]
    pub fn lookup(self, table: Vec<(&str, &str)>) -> Self {
        Expr::Lookup {
            target: Box::new(self),
            table: table
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        }
    }
    #[inline]
    pub fn if_then_else(condition: Expr, then: Expr, else_: Expr) -> Self {
        Expr::If {
            condition: Box::new(condition),
            then: Box::new(then),
            else_: Box::new(else_),
        }
    }
    #[inline]
    pub fn let_bind(name: impl Into<String>, value: Expr, body: Expr) -> Self {
        Expr::Let {
            name: name.into(),
            value: Box::new(value),
            body: Box::new(body),
        }
    }

    // ── List / collection methods ────────────────────────────────────────────
    #[inline]
    pub fn map(self, transform: Expr) -> Self {
        Expr::Map {
            target: Box::new(self),
            transform: Box::new(transform),
        }
    }
    #[inline]
    pub fn flat_map(self, transform: Expr) -> Self {
        Expr::FlatMap {
            target: Box::new(self),
            transform: Box::new(transform),
        }
    }
    #[inline]
    pub fn filter(self, predicate: Expr) -> Self {
        Expr::Filter {
            target: Box::new(self),
            filter: Box::new(predicate),
        }
    }
    #[inline]
    pub fn fold(self, base: Expr, transform: Expr) -> Self {
        Expr::Fold {
            target: Box::new(self),
            base: Box::new(base),
            transform: Box::new(transform),
        }
    }
    #[inline]
    pub fn join(self, delimiter: impl Into<String>) -> Self {
        Expr::Join {
            target: Box::new(self),
            delimiter: delimiter.into(),
        }
    }
    #[inline]
    pub fn resolve_url(self, base: Expr) -> Self {
        Expr::ResolveUrl {
            target: Box::new(self),
            base: Box::new(base),
        }
    }
    #[inline]
    pub fn list(items: Vec<Expr>) -> Self {
        Expr::List(items)
    }
    #[inline]
    pub fn concat(parts: Vec<Expr>) -> Self {
        Expr::Concat(parts)
    }

    // ── JSON-specific ────────────────────────────────────────────────────────
    #[inline]
    pub fn ptr(self, pointer: impl Into<String>) -> Self {
        Expr::JsonPtr {
            target: Box::new(self),
            pointer: pointer.into(),
        }
    }
    #[inline]
    pub fn str_val(self) -> Self {
        Expr::JsonStr {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn int_val(self) -> Self {
        Expr::JsonInt {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn float_val(self) -> Self {
        Expr::JsonFloat {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn bool_val(self) -> Self {
        Expr::JsonBool {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn array_len(self) -> Self {
        Expr::ArrayLen {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn keys(self) -> Self {
        Expr::JsonKeys {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn get(self, key: Expr) -> Self {
        Expr::JsonGet {
            target: Box::new(self),
            key: Box::new(key),
        }
    }
    #[inline]
    pub fn get_key(self, key: impl Into<String>) -> Self {
        self.get(Expr::Literal(key.into()))
    }
    #[inline]
    pub fn find(self, key: Expr, value: Expr) -> Self {
        Expr::JsonFind {
            target: Box::new(self),
            key: Box::new(key),
            value: Box::new(value),
        }
    }
    #[inline]
    pub fn find_kv(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.find(Expr::Literal(key.into()), Expr::Literal(value.into()))
    }
    #[inline]
    pub fn json_array(items: Vec<Expr>) -> Self {
        Expr::JsonArray(items)
    }
    #[inline]
    pub fn json_fold(self) -> Self {
        Expr::JsonFold {
            target: Box::new(self),
        }
    }

    #[inline]
    pub fn coalesce_keys(self, keys: impl IntoIterator<Item = Expr>) -> Self {
        let mut iter = keys.into_iter();
        let Some(first) = iter.next() else {
            return Expr::Null;
        };
        let base = self.clone().get(first).str_val();
        iter.fold(base, |acc, key| {
            acc.fallback(self.clone().get(key).str_val())
        })
    }

    // ── List merging ─────────────────────────────────────────────────────────
    #[inline]
    pub fn merge(lists: Vec<Expr>) -> Self {
        Expr::Merge(lists)
    }

    // ── Preferences ─────────────────────────────────────────────────────────
    #[inline]
    pub fn pref(key: impl Into<String>) -> Self {
        Expr::Pref(key.into())
    }

    // ── String formatting ────────────────────────────────────────────────────
    #[inline]
    pub fn format(template: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Format {
            template: template.into(),
            args,
        }
    }

    // ── Boolean / string extras ──────────────────────────────────────────────
    #[inline]
    pub fn not(self) -> Self {
        Expr::Not {
            target: Box::new(self),
        }
    }
    #[inline]
    pub fn is_null(self) -> Self {
        Expr::BinaryOperation {
            op: Op::Eq,
            lhs: Box::new(self),
            rhs: Box::from(Expr::Null),
        }
    }
    #[inline]
    pub fn string_len(self) -> Self {
        Expr::StringLen {
            target: Box::new(self),
        }
    }

    // ── Binary operators ─────────────────────────────────────────────────────
    #[inline]
    pub fn eq(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Eq,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn ne(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Ne,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn lt(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Lt,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn gt(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Gt,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn le(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Le,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn ge(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Ge,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn and(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::And,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn or(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Or,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn add(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Add,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn sub(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Sub,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn mul(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Mul,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
    #[inline]
    pub fn div(self, rhs: Expr) -> Self {
        Expr::BinaryOperation {
            op: Op::Div,
            lhs: Box::new(self),
            rhs: Box::new(rhs),
        }
    }
}

#[cfg(feature = "builder")]
pub struct BlueprintBuilder {
    request: Option<RequestDef>,
    container: String,
    fields: Vec<FieldDef>,
    bindings: Vec<Binding>,
    scalars: Vec<FieldDef>,
    pagination: Option<PaginationConfig>,
}

#[cfg(feature = "builder")]
impl BlueprintBuilder {
    pub fn new(container: impl Into<String>) -> Self {
        Self {
            request: None,
            container: container.into(),
            fields: vec![],
            bindings: vec![],
            scalars: vec![],
            pagination: None,
        }
    }
    pub fn field(mut self, name: &str, expr: Expr) -> Self {
        self.fields.push(FieldDef {
            name: name.into(),
            expr,
            optional: false,
        });
        self
    }
    pub fn field_opt(mut self, name: &str, expr: Expr) -> Self {
        self.fields.push(FieldDef {
            name: name.into(),
            expr,
            optional: true,
        });
        self
    }
    pub fn bind(mut self, name: &str, expr: Expr) -> Self {
        self.bindings.push(Binding {
            name: name.into(),
            expr,
        });
        self
    }
    pub fn scalar(mut self, name: &str, expr: Expr) -> Self {
        self.scalars.push(FieldDef {
            name: name.into(),
            expr,
            optional: false,
        });
        self
    }
    pub fn scalar_opt(mut self, name: &str, expr: Expr) -> Self {
        self.scalars.push(FieldDef {
            name: name.into(),
            expr,
            optional: true,
        });
        self
    }
    pub fn with_request(mut self, req: RequestDef) -> Self {
        self.request = Some(req);
        self
    }
    #[inline]
    pub fn paginated(
        mut self,
        native_page_size: usize,
        offset_param: impl Into<String>,
        offset_type: OffsetType,
    ) -> Self {
        self.pagination = Some(PaginationConfig {
            native_page_size,
            offset_param: offset_param.into(),
            offset_type,
        });
        self
    }
    pub fn build(self) -> Blueprint {
        Blueprint {
            request: self.request,
            container: self.container,
            fields: self.fields,
            bindings: self.bindings,
            scalars: self.scalars,
            pagination: self.pagination,
        }
    }
}

#[cfg(feature = "builder")]
impl Blueprint {
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Blueprint serialization failed")
    }

    pub fn with_request_def(&self, req: RequestDef) -> Blueprint {
        let mut bp = self.clone();
        bp.request = Some(req);
        bp
    }
}

#[cfg(all(test, feature = "builder"))]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::approx_constant)]
    use super::*;

    fn postcard_rt(expr: &Expr) {
        let bytes = postcard::to_allocvec(expr).unwrap();
        let back: Expr = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(*expr, back);
    }

    // ── Expr variants — postcard round-trip ──────────────────────────────────

    #[test]
    fn expr_postcard_round_trip_leaf_variants() {
        for e in &[
            Expr::SelfRef,
            Expr::Index,
            Expr::Null,
            Expr::Bool(true),
            Expr::Bool(false),
            Expr::Number(3.14),
            Expr::Number(-0.0),
            Expr::Literal("hello world".into()),
            Expr::Literal(String::new()),
            Expr::Var("base_url".into()),
            Expr::Dom("div.manga-title".into()),
            Expr::Json("/data/items".into()),
            Expr::Pref("lang".into()),
        ] {
            postcard_rt(e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_binary_operations_all_ops() {
        for op in &[
            Op::Add,
            Op::Sub,
            Op::Mul,
            Op::Div,
            Op::Eq,
            Op::Ne,
            Op::Lt,
            Op::Gt,
            Op::Le,
            Op::Ge,
            Op::And,
            Op::Or,
        ] {
            let e = Expr::BinaryOperation {
                op: op.clone(),
                lhs: Box::new(Expr::Number(1.0)),
                rhs: Box::new(Expr::Number(2.0)),
            };
            postcard_rt(&e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_html_methods() {
        for e in &[
            Expr::Attr {
                target: Box::new(Expr::SelfRef),
                name: "href".into(),
            },
            Expr::Text {
                target: Box::new(Expr::SelfRef),
            },
            Expr::InnerHtml {
                target: Box::new(Expr::SelfRef),
            },
            Expr::Select {
                target: Box::new(Expr::SelfRef),
                selector: "a.link".into(),
            },
            Expr::First {
                target: Box::new(Expr::SelfRef),
                selector: "span".into(),
            },
            Expr::HasClass {
                target: Box::new(Expr::SelfRef),
                class: "active".into(),
            },
            Expr::Children {
                target: Box::new(Expr::SelfRef),
            },
        ] {
            postcard_rt(e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_string_methods() {
        for e in &[
            Expr::Split {
                target: Box::new(Expr::Literal("a,b,c".into())),
                delimiter: ",".into(),
            },
            Expr::At {
                target: Box::new(Expr::SelfRef),
                index: 0,
            },
            Expr::At {
                target: Box::new(Expr::SelfRef),
                index: -1,
            },
            Expr::Replace {
                target: Box::new(Expr::Literal("hello".into())),
                from: "l".into(),
                to: "r".into(),
            },
            Expr::Trim {
                target: Box::new(Expr::SelfRef),
            },
            Expr::Lower {
                target: Box::new(Expr::SelfRef),
            },
            Expr::Matches {
                target: Box::new(Expr::SelfRef),
                pattern: r"^\d+$".into(),
            },
            Expr::Capture {
                target: Box::new(Expr::SelfRef),
                pattern: r"(\d+)".into(),
            },
            Expr::Prepend {
                target: Box::new(Expr::Literal("world".into())),
                prefix: Box::new(Expr::Literal("hello ".into())),
            },
            Expr::Append {
                target: Box::new(Expr::Literal("hello".into())),
                suffix: Box::new(Expr::Literal(" world".into())),
            },
            Expr::StartsWith {
                target: Box::new(Expr::Literal("hello".into())),
                prefix: "he".into(),
            },
            Expr::EndsWith {
                target: Box::new(Expr::Literal("hello".into())),
                suffix: "lo".into(),
            },
            Expr::Slice {
                target: Box::new(Expr::SelfRef),
                start: 0,
                end: Some(5),
            },
            Expr::Slice {
                target: Box::new(Expr::SelfRef),
                start: -3,
                end: None,
            },
            Expr::Join {
                target: Box::new(Expr::SelfRef),
                delimiter: ", ".into(),
            },
            Expr::StringLen {
                target: Box::new(Expr::Literal("hello".into())),
            },
            Expr::ToString {
                target: Box::new(Expr::Number(42.0)),
            },
        ] {
            postcard_rt(e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_parse_and_date() {
        for e in &[
            Expr::ParseFloat {
                target: Box::new(Expr::Literal("3.14".into())),
            },
            Expr::ParseInt {
                target: Box::new(Expr::Literal("42".into())),
            },
            Expr::DateParse {
                target: Box::new(Expr::SelfRef),
                format: "%Y-%m-%d".into(),
            },
            Expr::DateParseRfc3339 {
                target: Box::new(Expr::SelfRef),
            },
        ] {
            postcard_rt(e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_json_methods() {
        for e in &[
            Expr::JsonPtr {
                target: Box::new(Expr::Json("/".into())),
                pointer: "/key".into(),
            },
            Expr::JsonStr {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonInt {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonFloat {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonBool {
                target: Box::new(Expr::SelfRef),
            },
            Expr::ArrayLen {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonKeys {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonGet {
                target: Box::new(Expr::SelfRef),
                key: Box::new(Expr::Literal("k".into())),
            },
            Expr::JsonFind {
                target: Box::new(Expr::SelfRef),
                key: Box::new(Expr::Literal("id".into())),
                value: Box::new(Expr::Literal("1".into())),
            },
            Expr::JsonFold {
                target: Box::new(Expr::SelfRef),
            },
            Expr::JsonArray(vec![Expr::Number(1.0), Expr::Null, Expr::Bool(false)]),
        ] {
            postcard_rt(e);
        }
    }

    #[test]
    fn expr_postcard_round_trip_control_flow_and_collections() {
        for e in &[
            Expr::Let {
                name: "x".into(),
                value: Box::new(Expr::Number(1.0)),
                body: Box::new(Expr::Var("x".into())),
            },
            Expr::Fallback {
                target: Box::new(Expr::Null),
                default: Box::new(Expr::Literal("default".into())),
            },
            Expr::Lookup {
                target: Box::new(Expr::SelfRef),
                table: vec![("a".into(), "A".into()), ("b".into(), "B".into())],
            },
            Expr::If {
                condition: Box::new(Expr::Bool(true)),
                then: Box::new(Expr::Number(1.0)),
                else_: Box::new(Expr::Number(0.0)),
            },
            Expr::Not {
                target: Box::new(Expr::Bool(false)),
            },
            Expr::Map {
                target: Box::new(Expr::SelfRef),
                transform: Box::new(Expr::Text {
                    target: Box::new(Expr::SelfRef),
                }),
            },
            Expr::FlatMap {
                target: Box::new(Expr::SelfRef),
                transform: Box::new(Expr::SelfRef),
            },
            Expr::Filter {
                target: Box::new(Expr::SelfRef),
                filter: Box::new(Expr::Bool(true)),
            },
            Expr::Fold {
                target: Box::new(Expr::SelfRef),
                transform: Box::new(Expr::SelfRef),
                base: Box::new(Expr::Literal(String::new())),
            },
            Expr::List(vec![Expr::Number(1.0), Expr::Number(2.0)]),
            Expr::List(vec![]),
            Expr::Concat(vec![Expr::Literal("a".into()), Expr::Literal("b".into())]),
            Expr::Merge(vec![Expr::SelfRef, Expr::SelfRef]),
            Expr::ResolveUrl {
                target: Box::new(Expr::Literal("/page".into())),
                base: Box::new(Expr::Literal("https://example.com".into())),
            },
            Expr::Format {
                template: "Hello {}!".into(),
                args: vec![Expr::Literal("world".into())],
            },
        ] {
            postcard_rt(e);
        }
    }

    // ── Blueprint round-trip and builder ─────────────────────────────────────

    #[test]
    fn blueprint_postcard_round_trip_all_fields_populated() {
        let bp = BlueprintBuilder::new(".manga-list .item")
            .bind("base", Expr::dom("meta[property='og:url']").attr("content"))
            .field("id", Expr::self_ref().attr("data-id"))
            .field_opt("cover", Expr::self_ref().first("img").attr("src"))
            .scalar("total", Expr::dom(".pagination").text().parse_int())
            .scalar_opt("has_next", Expr::Null)
            .with_request(RequestDef {
                url: "https://example.com/popular".into(),
                method: "GET".into(),
                headers: vec![("Accept".into(), "text/html".into())],
                queries: vec![("page".into(), "1".into())],
            })
            .paginated(32, "offset", OffsetType::ItemOffset)
            .build();

        let bytes = postcard::to_allocvec(&bp).unwrap();
        let back: Blueprint = postcard::from_bytes(&bytes).unwrap();

        assert_eq!(bp.container, back.container);
        assert_eq!(bp.fields.len(), back.fields.len());
        assert_eq!(bp.bindings.len(), back.bindings.len());
        assert_eq!(bp.scalars.len(), back.scalars.len());

        let req = back.request.as_ref().unwrap();
        assert_eq!(req.url, "https://example.com/popular");
        assert_eq!(req.method, "GET");
        assert_eq!(req.headers, vec![("Accept".into(), "text/html".into())]);
        assert_eq!(req.queries, vec![("page".into(), "1".into())]);

        let pg = back.pagination.as_ref().unwrap();
        assert_eq!(pg.native_page_size, 32);
        assert_eq!(pg.offset_param, "offset");
        assert_eq!(pg.offset_type, OffsetType::ItemOffset);
    }

    #[test]
    fn blueprint_postcard_round_trip_minimal() {
        let bp = BlueprintBuilder::new("").build();
        let bytes = postcard::to_allocvec(&bp).unwrap();
        let back: Blueprint = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(bp.container, back.container);
        assert!(back.request.is_none());
        assert!(back.pagination.is_none());
        assert!(back.fields.is_empty());
    }

    #[test]
    fn blueprint_builder_produces_correct_structure() {
        let bp = BlueprintBuilder::new(".card")
            .field("id", Expr::self_ref().attr("data-id"))
            .field_opt("subtitle", Expr::Null)
            .bind("host", Expr::lit("https://example.com"))
            .scalar("count", Expr::num(42.0))
            .scalar_opt("page", Expr::Null)
            .build();

        assert_eq!(bp.container, ".card");
        assert_eq!(bp.fields.len(), 2);
        assert_eq!(bp.fields[0].name, "id");
        assert!(!bp.fields[0].optional);
        assert_eq!(bp.fields[1].name, "subtitle");
        assert!(bp.fields[1].optional);
        assert_eq!(bp.bindings.len(), 1);
        assert_eq!(bp.bindings[0].name, "host");
        assert_eq!(bp.scalars.len(), 2);
        assert_eq!(bp.scalars[0].name, "count");
        assert!(!bp.scalars[0].optional);
        assert!(bp.scalars[1].optional);
        assert!(bp.request.is_none());
        assert!(bp.pagination.is_none());
    }

    #[test]
    fn blueprint_to_bytes_is_deterministic() {
        let bp = BlueprintBuilder::new(".item")
            .field("title", Expr::self_ref().text().trim())
            .field("url", Expr::self_ref().first("a").attr("href"))
            .build();
        assert_eq!(bp.to_bytes(), bp.to_bytes());
    }

    #[test]
    fn with_request_def_only_changes_request_field() {
        let bp = BlueprintBuilder::new(".item")
            .field("title", Expr::self_ref().text())
            .bind("x", Expr::Null)
            .scalar("n", Expr::num(1.0))
            .paginated(10, "page", OffsetType::PageNumber { start: 1 })
            .build();

        let req = RequestDef {
            url: "https://test.com/list".into(),
            method: "POST".into(),
            headers: vec![],
            queries: vec![],
        };
        let bp2 = bp.with_request_def(req);

        let r = bp2.request.as_ref().unwrap();
        assert_eq!(r.url, "https://test.com/list");
        assert_eq!(r.method, "POST");
        assert_eq!(bp2.container, ".item");
        assert_eq!(bp2.fields.len(), 1);
        assert_eq!(bp2.bindings.len(), 1);
        assert_eq!(bp2.scalars.len(), 1);
        let pg = bp2.pagination.as_ref().unwrap();
        assert_eq!(pg.offset_type, OffsetType::PageNumber { start: 1 });
    }

    // ── PaginationConfig postcard round-trip ──────────────────────────────────

    #[test]
    fn pagination_config_postcard_round_trip_both_offset_types() {
        for cfg in &[
            PaginationConfig {
                native_page_size: 32,
                offset_param: "offset".into(),
                offset_type: OffsetType::ItemOffset,
            },
            PaginationConfig {
                native_page_size: 20,
                offset_param: "page".into(),
                offset_type: OffsetType::PageNumber { start: 1 },
            },
            PaginationConfig {
                native_page_size: 50,
                offset_param: "p".into(),
                offset_type: OffsetType::PageNumber { start: 0 },
            },
        ] {
            let bytes = postcard::to_allocvec(cfg).unwrap();
            let back: PaginationConfig = postcard::from_bytes(&bytes).unwrap();
            assert_eq!(*cfg, back);
        }
    }
}
