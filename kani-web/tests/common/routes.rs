//! Enumerates the REST surface by reading the routers, so contract tests cover
//! every route rather than the subset someone remembered to write a test for.
//!
//! Parsing the source rather than the axum `Router` is deliberate: a built router
//! does not expose its paths, and the OpenAPI document is a separate artifact that
//! can itself drift — proving they agree is what `openapi_coverage_tests` does.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

/// `(path, method)`, e.g. `("/rest/manga/{id}", "get")`.
pub type Route = (String, String);

pub const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

fn rest_sources() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/rest");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    out.sort();
    assert!(
        out.len() > 15,
        "expected the REST modules under {}, found {}",
        dir.display(),
        out.len()
    );
    out
}

/// Everything `.route("…", get(…).post(…))` mounts, prefixed with the `/rest`
/// nest from `app.rs`, mapped to the module that declares it.
pub fn declared_routes() -> BTreeMap<Route, String> {
    let mut out = BTreeMap::new();
    for file in rest_sources() {
        let src = std::fs::read_to_string(&file).unwrap();
        let module = file.file_stem().unwrap().to_string_lossy().to_string();
        for (path, chain) in route_calls(&src) {
            for method in METHODS {
                if mentions_method(&chain, method) {
                    out.insert((format!("/rest{path}"), method.to_string()), module.clone());
                }
            }
        }
    }
    out
}

/// The `("/path", <handler chain>)` pairs of every `.route(...)` call.
pub fn route_calls(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut cursor = 0usize;
    while let Some(rel) = src[cursor..].find(".route(") {
        let open = cursor + rel + ".route(".len();
        cursor = open;
        let rest = src[open..].trim_start();
        let Some(after_quote) = rest.strip_prefix('"') else {
            continue;
        };
        let Some(end) = after_quote.find('"') else {
            continue;
        };
        let path = &after_quote[..end];
        // The handler chain runs from the comma to the `.route(`'s closing paren.
        let mut depth = 1i32;
        let mut i = open;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
        let chain = &src[open..i.saturating_sub(1)];
        let after_path = chain.find(',').map(|c| &chain[c..]).unwrap_or("");
        out.push((path.to_string(), after_path.to_string()));
    }
    out
}

/// `get(...)` / `routing::get(...)`, but not `.get_ref(` or a word ending in it.
pub fn mentions_method(chain: &str, method: &str) -> bool {
    let mut cursor = 0usize;
    while let Some(rel) = chain[cursor..].find(method) {
        let at = cursor + rel;
        cursor = at + method.len();
        let before_ok = chain[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != '_');
        let after_ok = chain[cursor..].trim_start().starts_with('(');
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Substitutes a concrete value for every `{param}` so the path can be requested.
/// The value need not exist: the guard under test runs before the handler looks.
pub fn concrete_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + end];
        out.push_str(placeholder(name));
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Most path parameters are numeric ids; the rest are opaque strings.
fn placeholder(name: &str) -> &'static str {
    match name {
        "host" | "domain" => "example.com",
        "slug" | "name" | "key" | "kind" | "provider" | "token" => "placeholder",
        _ => "1",
    }
}
