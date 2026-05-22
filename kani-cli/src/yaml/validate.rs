//! Semantic validation: DSL parsing, variable references, and required field checks.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use chumsky::Parser;
use kani_shared::ast::Expr;

use super::model::{
    FieldSource, QueryEntry, QueryValue, ValidatedBinding, ValidatedEndpoint, ValidatedExtension,
    ValidatedField, ValidatedHnp, ValidatedPopular, ValidatedTotalPages,
};
use super::schema::{
    EndpointBody, FilterEntry, FilterMappingEntry, HasNextPage, PopularEndpoint, TotalPages,
    YamlExtension,
};
use crate::dsl::parser as dsl_parser;
use crate::error::{CliError, report_custom_error, report_errors};

const POPULAR_ARGS: &[&str] = &["page", "page_size", "filters"];
const SEARCH_ARGS: &[&str] = &["query", "page", "page_size", "filters"];
const DETAILS_ARGS: &[&str] = &["manga_id"];
const CHAPTER_LIST_ARGS: &[&str] = &["manga_id", "page", "page_size"];
const PAGES_ARGS: &[&str] = &["chapter_id", "manga_id"];

const MANGA_LIST_REQUIRED: &[&str] = &["id", "title"];
const DETAILS_REQUIRED: &[&str] = &["id", "title", "status"];
const CHAPTER_LIST_REQUIRED: &[&str] = &["id"];
const PAGES_REQUIRED: &[&str] = &["url", "index"];

pub fn validate(
    ext: &YamlExtension,
    _source: &str,
    path: &Path,
) -> Result<ValidatedExtension, Vec<CliError>> {
    let mut errors: Vec<CliError> = Vec::new();
    let filename = path.to_string_lossy().into_owned();

    let popular = ext.endpoints.popular.as_ref().and_then(|p| {
        match validate_popular(p, &filename, &ext.filters) {
            Ok(vp) => Some(vp),
            Err(mut errs) => {
                errors.append(&mut errs);
                None
            }
        }
    });

    let search = ext.endpoints.search.as_ref().and_then(|e| {
        match validate_endpoint(e, "search", SEARCH_ARGS, MANGA_LIST_REQUIRED, &filename) {
            Ok(ve) => Some(ve),
            Err(mut errs) => {
                errors.append(&mut errs);
                None
            }
        }
    });

    let manga_details =
        ext.endpoints.manga_details.as_ref().and_then(|e| {
            match validate_endpoint(
                e,
                "manga_details",
                DETAILS_ARGS,
                DETAILS_REQUIRED,
                &filename,
            ) {
                Ok(ve) => Some(ve),
                Err(mut errs) => {
                    errors.append(&mut errs);
                    None
                }
            }
        });

    let chapter_list = ext.endpoints.chapter_list.as_ref().and_then(|e| {
        match validate_endpoint(
            e,
            "chapter_list",
            CHAPTER_LIST_ARGS,
            CHAPTER_LIST_REQUIRED,
            &filename,
        ) {
            Ok(ve) => Some(ve),
            Err(mut errs) => {
                errors.append(&mut errs);
                None
            }
        }
    });

    let pages = ext.endpoints.pages.as_ref().and_then(|e| {
        match validate_endpoint(e, "pages", PAGES_ARGS, PAGES_REQUIRED, &filename) {
            Ok(ve) => Some(ve),
            Err(mut errs) => {
                errors.append(&mut errs);
                None
            }
        }
    });

    if errors.is_empty() {
        Ok(ValidatedExtension {
            id: ext.id.clone(),
            name: ext.name.clone(),
            version: ext.version.clone(),
            base_url: ext.base_url.clone(),
            language: ext.language.clone(),
            nsfw: ext.nsfw,
            unrestricted_http: ext.unrestricted_http,
            popular,
            search,
            manga_details,
            chapter_list,
            pages,
            filters: ext.filters.clone(),
            preferences: ext.preferences.clone(),
            get_url: ext.get_url.clone(),
        })
    } else {
        Err(errors)
    }
}

fn validate_popular(
    popular: &PopularEndpoint,
    filename: &str,
    _filters: &[FilterEntry],
) -> Result<ValidatedPopular, Vec<CliError>> {
    match popular {
        PopularEndpoint::Delegated {
            delegate_to,
            empty_without_filters,
        } => {
            let valid = ["search", "manga_details", "chapter_list", "pages"];
            if !valid.contains(&delegate_to.as_str()) {
                return Err(vec![CliError::Other(format!(
                    "popular.delegate_to '{}' must be one of: search, manga_details, chapter_list, pages",
                    delegate_to
                ))]);
            }
            Ok(ValidatedPopular::Delegated {
                delegate_to: delegate_to.clone(),
                empty_without_filters: *empty_without_filters,
            })
        }
        PopularEndpoint::Full(body) => {
            let endpoint =
                validate_endpoint(body, "popular", POPULAR_ARGS, MANGA_LIST_REQUIRED, filename)?;
            Ok(ValidatedPopular::Full(Box::new(endpoint)))
        }
    }
}

fn validate_endpoint(
    body: &EndpointBody,
    name: &str,
    fn_args: &[&str],
    required_fields: &[&str],
    filename: &str,
) -> Result<ValidatedEndpoint, Vec<CliError>> {
    let mut errors: Vec<CliError> = Vec::new();

    let route = match &body.route {
        Some(r) => {
            let mut route_errs = validate_route_vars(r, name, fn_args);
            errors.append(&mut route_errs);
            r.clone()
        }
        None => {
            errors.push(CliError::Other(format!(
                "endpoints.{name}: 'route' is required"
            )));
            String::new()
        }
    };

    let container = body
        .container
        .clone()
        .unwrap_or_else(|| ":root".to_string());

    let headers: Vec<(String, String)> = body
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let queries = match build_query_entries(&body.queries, name, fn_args) {
        Ok(q) => q,
        Err(mut errs) => {
            errors.append(&mut errs);
            vec![]
        }
    };

    let filter_mapping: Vec<(String, FilterMappingEntry)> = body
        .filter_mapping
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    for (key, entry) in &filter_mapping {
        if let FilterMappingEntry::SortPair { key_template, .. } = entry
            && !key_template.contains("{}")
        {
            errors.push(CliError::Other(format!(
                "endpoints.{name}.filter_mapping.{key}: sort_pair key_template must contain '{{}}'"
            )));
        }
    }

    let mut bindings = Vec::new();
    for (var_name, dsl) in &body.bindings {
        let field_path = format!("{filename}:endpoints.{name}.bindings.{var_name}");
        match parse_dsl(dsl, &field_path) {
            Ok(expr) => bindings.push(ValidatedBinding {
                name: var_name.clone(),
                expr,
            }),
            Err(mut errs) => errors.append(&mut errs),
        }
    }

    let mut fields = Vec::new();
    let mut present_fields: HashSet<&str> = HashSet::new();
    for (field_name, def) in &body.fields {
        present_fields.insert(field_name.as_str());
        let dsl = def.expr_str();
        let optional = def.optional();
        let field_path = format!("{filename}:endpoints.{name}.fields.{field_name}");

        let source = if let Some(arg_name) = extract_fn_arg_literal(dsl) {
            if !fn_args.contains(&arg_name.as_str()) {
                errors.push(CliError::Other(format!(
                    "endpoints.{name}.fields.{field_name}: '${arg_name}$' is not available \
                     (available args: {})",
                    fn_args.join(", ")
                )));
                continue;
            }
            FieldSource::FnArg(arg_name)
        } else {
            match parse_dsl(dsl, &field_path) {
                Ok(expr) => FieldSource::Blueprint(expr),
                Err(mut errs) => {
                    errors.append(&mut errs);
                    continue;
                }
            }
        };

        fields.push(ValidatedField {
            name: field_name.clone(),
            source,
            optional,
        });
    }

    for req in required_fields {
        if !present_fields.contains(*req) {
            errors.push(CliError::Other(format!(
                "endpoints.{name}: required field '{req}' is missing"
            )));
        }
    }

    let mut scalars = Vec::new();
    for (scalar_name, def) in &body.scalars {
        let dsl = def.expr_str();
        let optional = def.optional();
        let field_path = format!("{filename}:endpoints.{name}.scalars.{scalar_name}");

        let source = if let Some(arg_name) = extract_fn_arg_literal(dsl) {
            FieldSource::FnArg(arg_name)
        } else {
            match parse_dsl(dsl, &field_path) {
                Ok(expr) => FieldSource::Blueprint(expr),
                Err(mut errs) => {
                    errors.append(&mut errs);
                    continue;
                }
            }
        };

        scalars.push(ValidatedField {
            name: scalar_name.clone(),
            source,
            optional,
        });
    }

    let has_next_page = match &body.has_next_page {
        None => ValidatedHnp::Default,
        Some(HasNextPage::Static(b)) => ValidatedHnp::Static(*b),
        Some(HasNextPage::Expr(dsl)) => {
            let field_path = format!("{filename}:endpoints.{name}.has_next_page");
            match parse_dsl(dsl, &field_path) {
                Ok(expr) => ValidatedHnp::Scalar(expr),
                Err(mut errs) => {
                    errors.append(&mut errs);
                    ValidatedHnp::Default
                }
            }
        }
    };

    let total_pages = match &body.total_pages {
        None => ValidatedTotalPages::None,
        Some(TotalPages::Static(n)) => ValidatedTotalPages::Static(*n),
        Some(TotalPages::Expr(dsl)) => {
            let field_path = format!("{filename}:endpoints.{name}.total_pages");
            match parse_dsl(dsl, &field_path) {
                Ok(expr) => ValidatedTotalPages::Scalar(expr),
                Err(mut errs) => {
                    errors.append(&mut errs);
                    ValidatedTotalPages::None
                }
            }
        }
    };

    if errors.is_empty() {
        Ok(ValidatedEndpoint {
            route,
            method: body.method.clone(),
            headers,
            queries,
            filter_mapping,
            response_type: body.response_type,
            container,
            bindings,
            fields,
            scalars,
            has_next_page,
            total_pages,
            pagination: body.pagination.clone(),
        })
    } else {
        Err(errors)
    }
}

/// Replaces `\n + optional_hws + '.'` with `' .'` so multiline method chains
/// are joinable by the horizontal-only whitespace parser before `.`.
/// Preserves bare newlines (used as `let` terminators) when not followed by `.`.
fn normalize_multiline_chain(dsl: &str) -> String {
    let mut result = String::with_capacity(dsl.len());
    let bytes = dsl.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'.' {
                result.push(' ');
                i = j;
            } else {
                result.push('\n');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

fn parse_dsl(dsl: &str, field_path: &str) -> Result<Expr, Vec<CliError>> {
    let normalized = normalize_multiline_chain(dsl.trim_end());
    let result = dsl_parser().parse(normalized.as_str());

    if result.has_errors() {
        let errs: Vec<_> = result.errors().cloned().collect();
        report_errors(field_path, &normalized, errs);
        return Err(vec![CliError::Other(format!(
            "DSL parse failed in {field_path}"
        ))]);
    }

    let parse_ast = result
        .into_result()
        .map_err(|_| vec![CliError::Other(format!("DSL parse failed in {field_path}"))])?;

    let expr: Result<Expr, Vec<CliError>> = parse_ast.try_into();

    expr.inspect_err(|errs| {
        for e in errs {
            if let CliError::DslConversion { message, span } = e {
                report_custom_error(field_path, &normalized, message, span.clone());
            }
        }
    })
}

/// Detects `"$varname$"` — a DSL string literal wrapping a dollar-fenced identifier.
/// These are emitted as direct function-arg passthroughs rather than blueprint fields.
fn extract_fn_arg_literal(dsl: &str) -> Option<String> {
    let trimmed = dsl.trim();
    if trimmed.len() > 4 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.starts_with('$') && inner.ends_with('$') && inner.len() > 2 {
            let name = &inner[1..inner.len() - 1];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extracts all `$varname$` placeholders from a route or query string.
fn extract_dollar_vars(s: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'$' && end > start {
                vars.push(s[start..end].to_string());
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    vars
}

fn validate_route_vars(route: &str, endpoint: &str, fn_args: &[&str]) -> Vec<CliError> {
    extract_dollar_vars(route)
        .into_iter()
        .filter(|v| !fn_args.contains(&v.as_str()))
        .map(|v| {
            CliError::Other(format!(
                "endpoints.{endpoint}.route: '${v}$' is not available \
             (available args: {})",
                fn_args.join(", ")
            ))
        })
        .collect()
}

fn build_query_entries(
    queries: &BTreeMap<String, String>,
    endpoint: &str,
    fn_args: &[&str],
) -> Result<Vec<QueryEntry>, Vec<CliError>> {
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for (key, value) in queries {
        let trimmed = value.trim();
        let query_value = if trimmed.starts_with('$') && trimmed.ends_with('$') && trimmed.len() > 2
        {
            let var = &trimmed[1..trimmed.len() - 1];
            if !fn_args.contains(&var) {
                errors.push(CliError::Other(format!(
                    "endpoints.{endpoint}.queries.{key}: '${var}$' is not available \
                     (available args: {})",
                    fn_args.join(", ")
                )));
                continue;
            }
            QueryValue::Arg(var.to_string())
        } else {
            QueryValue::Static(value.clone())
        };
        entries.push(QueryEntry {
            key: key.clone(),
            value: query_value,
        });
    }

    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}
