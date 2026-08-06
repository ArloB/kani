#![allow(clippy::unwrap_used)]

//! The published API surface has to match the router. Two things drift silently:
//! a handler that never gets a `#[utoipa::path]`, and — less obviously — one
//! that has the annotation but was never added to `ApiDoc`'s `paths(...)` list,
//! which produces no spec entry at all. Both look fine in review.
//!
//! So the comparison is against the generated document, not against the
//! presence of an attribute: whatever `ApiDoc::openapi()` emits is what clients
//! get.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use utoipa::OpenApi;

/// `(path, method)`, e.g. `("/rest/manga/{id}", "get")`.
type Route = (String, String);

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// Routes the surface deliberately leaves out of the spec.
///
/// Keep this short and justified — an entry here is a documented endpoint that
/// clients cannot discover.
fn exempt(path: &str, _method: &str) -> Option<&'static str> {
    match path {
        // Server-sent events. OpenAPI describes request/response pairs; an
        // endless `text/event-stream` is not one, and the event payloads are
        // contract-tested in `sse_contract_tests.rs` instead.
        "/rest/events" => Some("SSE stream, not a request/response endpoint"),
        _ => None,
    }
}

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
/// nest from `app.rs`.
fn declared_routes() -> BTreeMap<Route, String> {
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
fn route_calls(src: &str) -> Vec<(String, String)> {
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
fn mentions_method(chain: &str, method: &str) -> bool {
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

/// What clients actually receive.
fn documented_routes() -> BTreeSet<Route> {
    let doc = kani_web::openapi::ApiDoc::openapi();
    let mut out = BTreeSet::new();
    for (path, item) in doc.paths.paths.iter() {
        for (method, present) in [
            ("get", item.get.is_some()),
            ("post", item.post.is_some()),
            ("put", item.put.is_some()),
            ("patch", item.patch.is_some()),
            ("delete", item.delete.is_some()),
            ("head", item.head.is_some()),
            ("options", item.options.is_some()),
        ] {
            if present {
                out.insert((path.clone(), method.to_string()));
            }
        }
    }
    out
}

#[test]
fn every_rest_route_appears_in_the_openapi_document() {
    let declared = declared_routes();
    let documented = documented_routes();

    assert!(
        declared.len() > 200,
        "the route scan found only {} routes, so it is not parsing the modules",
        declared.len()
    );

    let mut missing: Vec<String> = declared
        .iter()
        .filter(|((path, method), _)| {
            !documented.contains(&(path.clone(), method.clone())) && exempt(path, method).is_none()
        })
        .map(|((path, method), module)| {
            format!("  {} {path}  ({module}.rs)", method.to_uppercase())
        })
        .collect();
    missing.sort();

    assert!(
        missing.is_empty(),
        "{} route(s) are served but absent from the OpenAPI document — either the \
         handler has no #[utoipa::path], or it has one but was never added to \
         ApiDoc's paths(...) list, which produces no spec entry either way:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn the_document_describes_nothing_the_router_does_not_serve() {
    let declared = declared_routes();
    let documented = documented_routes();

    let mut phantom: Vec<String> = documented
        .iter()
        .filter(|route| !declared.contains_key(*route))
        .map(|(path, method)| format!("  {} {path}", method.to_uppercase()))
        .collect();
    phantom.sort();

    assert!(
        phantom.is_empty(),
        "the OpenAPI document promises {} endpoint(s) the router does not serve — a \
         client following the spec gets a 404:\n{}",
        phantom.len(),
        phantom.join("\n")
    );
}

#[test]
fn the_scan_recognises_the_shapes_the_routers_use() {
    let sample = r#"
        Router::new()
            .route("/settings", get(get_settings).patch(update_settings))
            .route("/manga/{id}", axum::routing::delete(remove))
            .route("/x", post(create))
    "#;
    let calls = route_calls(sample);
    assert_eq!(calls.len(), 3, "one entry per .route() call");

    assert!(mentions_method(&calls[0].1, "get"));
    assert!(mentions_method(&calls[0].1, "patch"));
    assert!(!mentions_method(&calls[0].1, "post"));
    assert!(mentions_method(&calls[1].1, "delete"));
    assert!(
        !mentions_method(&calls[1].1, "get"),
        "the path is not a method"
    );
    assert!(mentions_method(&calls[2].1, "post"));

    assert!(!mentions_method(", forget(handler)", "get"));
}

#[test]
fn every_operation_is_tagged_with_a_declared_tag() {
    let doc = kani_web::openapi::ApiDoc::openapi();
    let declared: BTreeSet<String> = doc.tags.iter().flatten().map(|t| t.name.clone()).collect();

    let mut problems = Vec::new();
    for (path, item) in doc.paths.paths.iter() {
        let operations = [
            ("get", &item.get),
            ("post", &item.post),
            ("put", &item.put),
            ("patch", &item.patch),
            ("delete", &item.delete),
            ("head", &item.head),
            ("options", &item.options),
        ];
        for (method, op) in operations {
            let Some(op) = op else { continue };
            let tags = op.tags.clone().unwrap_or_default();
            if tags.is_empty() {
                problems.push(format!("  {} {path} has no tag", method.to_uppercase()));
            }
            for tag in tags {
                if !declared.contains(&tag) {
                    problems.push(format!(
                        "  {} {path} is tagged \"{tag}\", which ApiDoc's tags(...) does not declare",
                        method.to_uppercase()
                    ));
                }
            }
            if op.responses.responses.is_empty() {
                problems.push(format!(
                    "  {} {path} documents no responses",
                    method.to_uppercase()
                ));
            }
        }
    }
    problems.sort();

    assert!(
        problems.is_empty(),
        "{} operation(s) are documented in a way clients cannot navigate:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

fn documented_operations() -> Vec<(String, String, Option<String>)> {
    let doc = kani_web::openapi::ApiDoc::openapi();
    let mut out = Vec::new();
    for (path, item) in doc.paths.paths.iter() {
        let operations = [
            ("get", &item.get),
            ("post", &item.post),
            ("put", &item.put),
            ("patch", &item.patch),
            ("delete", &item.delete),
            ("head", &item.head),
            ("options", &item.options),
        ];
        for (method, op) in operations {
            let Some(op) = op else { continue };
            let tier = op
                .extensions
                .as_ref()
                .and_then(|e| e.get("x-stability"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            out.push((path.clone(), method.to_string(), tier));
        }
    }
    out
}

#[test]
fn every_operation_publishes_a_stability_tier() {
    let mut problems: Vec<String> = documented_operations()
        .into_iter()
        .filter_map(|(path, method, tier)| match tier.as_deref() {
            Some("stable") | Some("unstable") => None,
            Some(other) => Some(format!(
                "  {} {path} declares unknown tier \"{other}\"",
                method.to_uppercase()
            )),
            None => Some(format!(
                "  {} {path} has no x-stability extension",
                method.to_uppercase()
            )),
        })
        .collect();
    problems.sort();

    assert!(
        problems.is_empty(),
        "{} operation(s) are published without a usable compatibility tier:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn the_tier_split_is_not_vacuous() {
    let ops = documented_operations();
    let stable = ops
        .iter()
        .filter(|(_, _, t)| t.as_deref() == Some("stable"))
        .count();
    let unstable = ops
        .iter()
        .filter(|(_, _, t)| t.as_deref() == Some("unstable"))
        .count();

    assert!(
        stable > 0 && unstable > 0,
        "the tier table must partition the surface, got {stable} stable and {unstable} unstable"
    );
    assert!(
        ops.len() > 200,
        "only {} operations enumerated, so the scan is not reading the document",
        ops.len()
    );
}

#[test]
fn administrative_and_internal_routes_are_never_promised_stable() {
    let mut leaked: Vec<String> = documented_operations()
        .into_iter()
        .filter(|(path, _, tier)| {
            let internal = path.starts_with("/rest/admin")
                || path.starts_with("/rest/ui")
                || path.starts_with("/rest/jobs")
                || path.starts_with("/rest/server");
            internal && tier.as_deref() != Some("unstable")
        })
        .map(|(path, method, tier)| format!("  {} {path} -> {:?}", method.to_uppercase(), tier))
        .collect();
    leaked.sort();

    assert!(
        leaked.is_empty(),
        "{} internal operation(s) would be frozen by the 1.x promise:\n{}",
        leaked.len(),
        leaked.join("\n")
    );
}

#[test]
fn the_serialised_document_carries_the_tier() {
    let json = kani_web::openapi::ApiDoc::openapi()
        .to_json()
        .expect("the document must serialise");
    let doc: serde_json::Value = serde_json::from_str(&json).unwrap();

    let admin = doc["paths"]["/rest/admin/users"]["get"]["x-stability"].as_str();
    let library = doc["paths"]["/rest/library"]["get"]["x-stability"].as_str();

    assert_eq!(
        admin,
        Some("unstable"),
        "GET /rest/admin/users must reach clients marked unstable"
    );
    assert_eq!(
        library,
        Some("stable"),
        "GET /rest/library must reach clients marked stable"
    );
}
