//! Guest-safe request construction shared by both YAML execution engines.
//!
//! The interpreted `YamlSource` and `kani-cli`'s codegen both take the same
//! declarative endpoint and must build the same HTTP request from it. Historically
//! each had its own copy of that logic, and they drifted (filter mapping missing on
//! one side, no placeholder encoding, literal `$page$` in URLs). This module is the
//! single implementation both lower into: it is `wasm32`-clean so the generated
//! guest code can call it directly, and native so the interpreter can.
//!
//! It covers the request *envelope* — URL substitution, static queries, and filter
//! mapping. Pagination offsets are added by the evaluator, and the extraction
//! result is unpacked separately.

use crate::types::{ActiveFilter, FilterState};
use std::collections::HashMap;

/// How a query parameter's value is sourced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryValue {
    /// A literal string, used verbatim.
    Static(String),
    /// A `$var$` placeholder resolved from the runtime args (by the dot-replaced key).
    Arg(String),
}

/// One declared query parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuerySpec {
    pub key: String,
    pub value: QueryValue,
}

/// How a filter group maps onto query parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterMapping {
    /// One group → one parameter.
    Simple(String),
    /// A `key:dir` selection → a templated key plus an optional direction param.
    SortPair {
        key_template: String,
        direction_param: Option<String>,
    },
    /// A `from:to` text input → two parameters.
    TupleSplit {
        from_param: String,
        to_param: String,
    },
}

/// How multiselect values are serialised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrayFormat {
    /// `tag=a&tag=b`
    #[default]
    Repeated,
    /// `tag[]=a&tag[]=b`
    Bracket,
    /// `tag=a,b` (joined by `array_separator`)
    CommaSeparated,
}

/// How a boolean checkbox value is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoolFormat {
    #[default]
    TrueFalse,
    OneZero,
    YesNo,
}

/// Presentation options for filter serialisation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterFormat {
    pub multiselect: ArrayFormat,
    pub omit_empty: bool,
    pub bool_format: BoolFormat,
    pub array_separator: String,
}

impl Default for FilterFormat {
    fn default() -> Self {
        Self {
            multiselect: ArrayFormat::default(),
            omit_empty: true,
            bool_format: BoolFormat::default(),
            array_separator: ",".to_string(),
        }
    }
}

/// Render a boolean as its configured literal.
pub fn bool_literal(fmt: BoolFormat, value: bool) -> &'static str {
    match (fmt, value) {
        (BoolFormat::TrueFalse, true) => "true",
        (BoolFormat::TrueFalse, false) => "false",
        (BoolFormat::OneZero, true) => "1",
        (BoolFormat::OneZero, false) => "0",
        (BoolFormat::YesNo, true) => "yes",
        (BoolFormat::YesNo, false) => "no",
    }
}

/// Substitute `$var$` placeholders in `route` from `args`, percent-encoding each
/// value as a single path segment, and prepend `base_url`.
///
/// A source-supplied value fills exactly one route slot, so it is encoded: an id
/// like `../admin`, `x?y=1` or `a b` must not smuggle in a path traversal, a
/// query, or a space. An unresolved placeholder is an error, never a literal
/// `$page$` on the wire. Composite-id sub-fields (`$manga.hid$`) are looked up by
/// the dot-replaced key (`manga_hid`).
pub fn build_url(
    base_url: &str,
    route: &str,
    args: &HashMap<String, String>,
) -> Result<String, String> {
    let mut result = String::with_capacity(route.len());
    let bytes = route.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'.')
            {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'$' && end > start {
                let placeholder = &route[start..end];
                let key = placeholder.replace('.', "_");
                match args.get(&key) {
                    Some(val) => result.push_str(&urlencoding::encode(val)),
                    None => {
                        return Err(format!(
                            "unresolved route placeholder `${placeholder}$` (no argument supplied)"
                        ));
                    }
                }
                i = end + 1;
            } else {
                result.push('$');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(format!("{}{}", base_url.trim_end_matches('/'), result))
}

/// Resolve declared query parameters against the runtime args.
pub fn build_queries(
    queries: &[QuerySpec],
    args: &HashMap<String, String>,
) -> Vec<(String, String)> {
    queries
        .iter()
        .filter_map(|e| {
            let val = match &e.value {
                QueryValue::Static(s) => s.clone(),
                QueryValue::Arg(name) => args.get(name.as_str())?.clone(),
            };
            Some((e.key.clone(), val))
        })
        .collect()
}

/// Map active filters onto query parameters per the endpoint's `filter_mapping`
/// and `filter_format`. The reference both engines share (was A1: absent from the
/// interpreter, re-emitted by codegen).
pub fn apply_filters(
    filter_mapping: &[(String, FilterMapping)],
    filter_format: Option<&FilterFormat>,
    filters: &[ActiveFilter],
) -> Vec<(String, String)> {
    let bool_fmt = filter_format.map(|f| f.bool_format).unwrap_or_default();
    let omit_empty = filter_format.map(|f| f.omit_empty).unwrap_or(true);
    let array_fmt = filter_format.map(|f| f.multiselect).unwrap_or_default();
    let array_sep = filter_format
        .map(|f| f.array_separator.as_str())
        .unwrap_or(",");

    let mut out: Vec<(String, String)> = Vec::new();

    for f in filters {
        // `group:action` — the action half lets one filter group carry a value in
        // its name, e.g. `genre:include`.
        let (group, action) = f
            .filter_name
            .split_once(':')
            .unwrap_or((f.filter_name.as_str(), ""));

        let Some((_, entry)) = filter_mapping.iter().find(|(k, _)| k == group) else {
            continue;
        };

        match entry {
            FilterMapping::Simple(param) => match &f.state {
                FilterState::Checkbox(true) => {
                    let v = if action.is_empty() {
                        bool_literal(bool_fmt, true).to_string()
                    } else {
                        action.to_string()
                    };
                    out.push((param.clone(), v));
                }
                FilterState::Checkbox(false) if !omit_empty => {
                    out.push((param.clone(), bool_literal(bool_fmt, false).to_string()));
                }
                FilterState::Multiselect(values) => match array_fmt {
                    ArrayFormat::Repeated => {
                        for v in values {
                            out.push((param.clone(), v.clone()));
                        }
                    }
                    ArrayFormat::Bracket => {
                        for v in values {
                            out.push((format!("{param}[]"), v.clone()));
                        }
                    }
                    ArrayFormat::CommaSeparated => {
                        out.push((param.clone(), values.join(array_sep)));
                    }
                },
                FilterState::Selection { value, .. } => out.push((param.clone(), value.clone())),
                FilterState::TextInput(s) => out.push((param.clone(), s.clone())),
                _ => {}
            },
            FilterMapping::SortPair {
                key_template,
                direction_param,
            } => {
                if let FilterState::Selection { value, .. } = &f.state
                    && let Some((key_part, dir)) = value.split_once(':')
                {
                    out.push((key_template.replace("{}", key_part), dir.to_string()));
                    if let Some(dir_param) = direction_param {
                        out.push((dir_param.clone(), dir.to_string()));
                    }
                }
            }
            FilterMapping::TupleSplit {
                from_param,
                to_param,
            } => {
                if let FilterState::TextInput(s) = &f.state
                    && let Some((from, to)) = s.split_once(':')
                {
                    out.push((from_param.clone(), from.to_string()));
                    out.push((to_param.clone(), to.to_string()));
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn build_url_encodes_a_traversal_into_one_segment() {
        let url = build_url(
            "https://s.example",
            "/manga/$manga_id$/x",
            &args(&[("manga_id", "../admin")]),
        )
        .unwrap();
        assert_eq!(url, "https://s.example/manga/..%2Fadmin/x");
    }

    #[test]
    fn build_url_errors_on_unresolved_placeholder() {
        let err = build_url("https://s.example", "/list/$page$", &args(&[])).unwrap_err();
        assert!(err.contains("page"));
    }

    #[test]
    fn apply_filters_maps_a_multiselect() {
        let mapping = vec![("genre".to_string(), FilterMapping::Simple("g".to_string()))];
        let filters = vec![ActiveFilter {
            filter_name: "genre".to_string(),
            state: FilterState::Multiselect(vec!["a".to_string(), "b".to_string()]),
        }];
        let out = apply_filters(&mapping, None, &filters);
        assert_eq!(
            out,
            vec![
                ("g".to_string(), "a".to_string()),
                ("g".to_string(), "b".to_string())
            ]
        );
    }
}
