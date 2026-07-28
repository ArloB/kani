#![allow(clippy::unwrap_used)]
#![allow(clippy::pedantic)]

//! Backend parity suite. Kani has two source backends — compiled WASM
//! (`WasmSource`) and interpreted YAML (`YamlSource`) — that share the Blueprint
//! evaluator but diverge in request construction, unpacking, and error kinds.
//! This suite points BOTH at the same `TestOrigin`, driving the compiled
//! `kani-fixture-source` and a YAML source configured with the identical HTML
//! contract, and asserts they produce the same result. It is how we keep the two
//! from silently drifting apart.
//!
//! The fixture WASM must be built first:
//!   cargo run -p kani-cli -- build kani-fixture-source

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::source::{SourceBackend, loader};
use kani_core::cache::InMemoryCache;
use kani_core::http::SmartClient;
use kani_core::wasm::WasmRuntime;
use kani_shared::ast::Expr;
use kani_shared::extension::ExtensionErrorKind;
use kani_shared_test::origin::{Response, TestOrigin};
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedPopular, ValidatedTotalPages,
};
use kani_yaml::yaml::schema::ResponseType;

// ── Shared HTML contract (identical selectors to the fixture extension) ────────

const POPULAR_HTML: &str = r#"<html><body>
    <div class="item" data-id="manga-1"><span class="title">First</span></div>
    <div class="item" data-id="manga-2"><span class="title">Second</span></div>
</body></html>"#;

const SEARCH_HTML: &str = r#"<html><body>
    <div class="item" data-id="s-1"><span class="title">Search Hit</span></div>
</body></html>"#;

const DETAILS_HTML: &str = r#"<html><body>
    <div class="manga" data-id="manga-1"><h1>My Title</h1></div>
</body></html>"#;

const CHAPTERS_HTML: &str = r#"<html><body>
    <div class="ch" data-id="ch-1"><span class="title">Chapter 1</span></div>
    <div class="ch" data-id="ch-2"><span class="title">Chapter 2</span></div>
</body></html>"#;

const PAGES_HTML: &str = r#"<html><body>
    <div class="page" data-url="https://cdn.example.com/p1.jpg"></div>
    <div class="page" data-url="https://cdn.example.com/p2.jpg"></div>
</body></html>"#;

fn seed_happy_path(origin: &TestOrigin) {
    origin.set("/popular", Response::html(POPULAR_HTML));
    origin.set("/search", Response::html(SEARCH_HTML));
    origin.set("/manga/manga-1", Response::html(DETAILS_HTML));
    origin.set("/manga/manga-1/chapters", Response::html(CHAPTERS_HTML));
    origin.set("/manga/manga-1/chapter/ch-1", Response::html(PAGES_HTML));
}

// ── Backend builders ──────────────────────────────────────────────────────────

fn fixture_wasm_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("wasm_sources")
        .join("fixture.wasm")
}

/// Builds the compiled-WASM backend from `fixture.wasm`, injecting the origin as
/// the `base_url` preference the guest reads to construct its requests. Returns
/// `None` (with a skip note) if the fixture has not been built.
fn wasm_backend(origin_base: &str) -> Option<SourceBackend> {
    let path = fixture_wasm_path();
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!(
            "\n[SKIP] {} not found — build it with: \
             cargo run -p kani-cli -- build kani-fixture-source\n",
            path.display()
        );
        return None;
    };

    let rt = WasmRuntime::new_on_demand().unwrap();
    let component = rt.compile_component(&bytes).unwrap();
    let instance_pre = rt.instantiate_pre(&component).unwrap();
    let engine = rt.engine().clone();

    let mut prefs = HashMap::new();
    prefs.insert("base_url".to_string(), origin_base.to_string());

    Some(loader::build_wasm_source(
        engine,
        instance_pre,
        SmartClient::new(None).unwrap(),
        None,
        true, // unrestricted_http → reach loopback
        false,
        prefs,
        Arc::new(InMemoryCache::new()),
        "fixture:".to_string(),
        None,
        None,
        0,
    ))
}

fn list_endpoint(route: &str, container: &str) -> ValidatedEndpoint {
    ValidatedEndpoint {
        route: route.to_string(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Html,
        container: container.to_string(),
        bindings: vec![],
        fields: vec![],
        scalars: vec![],
        has_next_page: ValidatedHnp::Static(false),
        total_pages: ValidatedTotalPages::None,
        pagination: None,
        composite_id_decodes: vec![],
        then_steps: vec![],
        for_each_steps: vec![],
        via: None,
        page_url: None,
        script_name: None,
        timeout_ms: 10_000,
    }
}

fn attr_field(name: &str, attr: &str) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(Expr::Attr {
            target: Box::new(Expr::SelfRef),
            name: attr.to_string(),
        }),
        optional: false,
    }
}

fn text_field(name: &str, selector: &str) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(Expr::Text {
            target: Box::new(Expr::First {
                target: Box::new(Expr::SelfRef),
                selector: selector.to_string(),
            }),
        }),
        optional: false,
    }
}

fn ep(route: &str, container: &str, fields: Vec<ValidatedField>) -> ValidatedEndpoint {
    ValidatedEndpoint {
        fields,
        ..list_endpoint(route, container)
    }
}

/// Builds the YAML backend with endpoints matching the fixture's contract 1:1.
fn yaml_backend(origin_base: &str) -> SourceBackend {
    let config = ValidatedExtension {
        id: "fixture".into(),
        name: "Fixture Source".into(),
        version: "1.0.0".into(),
        base_url: origin_base.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        popular: Some(ValidatedPopular::Full(Box::new(ep(
            "/popular",
            ".item",
            vec![attr_field("id", "data-id"), text_field("title", ".title")],
        )))),
        search: Some(ep(
            "/search",
            ".item",
            vec![attr_field("id", "data-id"), text_field("title", ".title")],
        )),
        manga_details: Some(ep(
            "/manga/$manga_id$",
            ".manga",
            vec![attr_field("id", "data-id"), text_field("title", "h1")],
        )),
        chapter_list: Some(ep(
            "/manga/$manga_id$/chapters",
            ".ch",
            vec![attr_field("id", "data-id"), text_field("title", ".title")],
        )),
        pages: Some(ep(
            "/manga/$manga_id$/chapter/$chapter_id$",
            ".page",
            vec![attr_field("url", "data-url")],
        )),
        ..Default::default()
    };

    loader::build_yaml_source(
        Arc::new(config),
        SmartClient::new(None).unwrap(),
        Arc::new(InMemoryCache::new()),
        "fixture:".into(),
        HashMap::new(),
        false,
    )
}

// ── Parity: happy paths ───────────────────────────────────────────────────────

macro_rules! wasm_or_skip {
    ($origin:expr) => {
        match wasm_backend(&$origin.base()) {
            Some(b) => b,
            None => return,
        }
    };
}

#[tokio::test]
async fn popular_parity() {
    let origin = TestOrigin::start().await;
    seed_happy_path(&origin);
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let w = wasm.get_popular_manga(1, 20, &[]).await.unwrap();
    let y = yaml.get_popular_manga(1, 20, &[]).await.unwrap();

    let wm: Vec<_> = w.manga.iter().map(|m| (&m.id, &m.title)).collect();
    let ym: Vec<_> = y.manga.iter().map(|m| (&m.id, &m.title)).collect();
    assert_eq!(wm, ym, "WASM and YAML popular results must match");
    assert_eq!(
        wm,
        vec![
            (&"manga-1".to_string(), &"First".to_string()),
            (&"manga-2".to_string(), &"Second".to_string())
        ]
    );
}

#[tokio::test]
async fn search_parity() {
    let origin = TestOrigin::start().await;
    seed_happy_path(&origin);
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let w = wasm.search_manga("anything", 1, 20, &[]).await.unwrap();
    let y = yaml.search_manga("anything", 1, 20, &[]).await.unwrap();

    let wm: Vec<_> = w.manga.iter().map(|m| (&m.id, &m.title)).collect();
    let ym: Vec<_> = y.manga.iter().map(|m| (&m.id, &m.title)).collect();
    assert_eq!(wm, ym);
    assert_eq!(w.manga.len(), 1);
    assert_eq!(w.manga[0].id, "s-1");
}

#[tokio::test]
async fn details_parity() {
    let origin = TestOrigin::start().await;
    seed_happy_path(&origin);
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let w = wasm.get_manga_details("manga-1").await.unwrap();
    let y = yaml.get_manga_details("manga-1").await.unwrap();

    assert_eq!((&w.id, &w.title), (&y.id, &y.title));
    assert_eq!(w.title, "My Title");
    assert_eq!(w.id, "manga-1");
}

#[tokio::test]
async fn chapter_list_parity() {
    let origin = TestOrigin::start().await;
    seed_happy_path(&origin);
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let w = wasm
        .get_chapter_list("manga-1", 1, None, None)
        .await
        .unwrap();
    let y = yaml
        .get_chapter_list("manga-1", 1, None, None)
        .await
        .unwrap();

    let wc: Vec<_> = w.chapters.iter().map(|c| &c.id).collect();
    let yc: Vec<_> = y.chapters.iter().map(|c| &c.id).collect();
    assert_eq!(wc, yc);
    assert_eq!(wc, vec!["ch-1", "ch-2"]);
}

#[tokio::test]
async fn pages_parity() {
    let origin = TestOrigin::start().await;
    seed_happy_path(&origin);
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let w = wasm.get_pages("manga-1", "ch-1").await.unwrap();
    let y = yaml.get_pages("manga-1", "ch-1").await.unwrap();

    let wu: Vec<_> = w.pages.iter().map(|p| (p.index, &p.url)).collect();
    let yu: Vec<_> = y.pages.iter().map(|p| (p.index, &p.url)).collect();
    assert_eq!(wu, yu);
    assert_eq!(w.pages.len(), 2);
    assert_eq!(w.pages[0].url, "https://cdn.example.com/p1.jpg");
}

// ── Parity: error kinds ───────────────────────────────────────────────────────
//
// A15 made the interpreted-YAML backend report typed HTTP error kinds. The
// compiled-WASM backend must not silently diverge back to a bare parse error —
// the evaluator's `__http_status__:` sentinel has to survive the guest boundary.

fn err_kind<T: std::fmt::Debug>(r: kani_core::error::Result<T>) -> ExtensionErrorKind {
    match r {
        Err(kani_core::error::Error::Extension(e)) => e.kind,
        other => panic!("expected an Extension error, got {other:?}"),
    }
}

#[tokio::test]
async fn error_kind_parity_503() {
    let origin = TestOrigin::start().await;
    origin.set("/popular", Response::status(503));
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let wk = err_kind(wasm.get_popular_manga(1, 20, &[]).await);
    let yk = err_kind(yaml.get_popular_manga(1, 20, &[]).await);
    assert_eq!(wk, yk, "a 5xx must classify identically in both backends");
    assert_eq!(yk, ExtensionErrorKind::Network);
}

#[tokio::test]
async fn error_kind_parity_429() {
    let origin = TestOrigin::start().await;
    origin.set("/popular", Response::status(429).header("Retry-After", "1"));
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    let wk = err_kind(wasm.get_popular_manga(1, 20, &[]).await);
    let yk = err_kind(yaml.get_popular_manga(1, 20, &[]).await);
    assert_eq!(wk, yk, "a 429 must classify identically in both backends");
    assert_eq!(yk, ExtensionErrorKind::RateLimited);
}

#[tokio::test]
async fn error_404_is_empty_in_both_backends() {
    let origin = TestOrigin::start().await;
    origin.set("/popular", Response::status(404));
    let wasm = wasm_or_skip!(origin);
    let yaml = yaml_backend(&origin.base());

    // 404 is deliberately not a typed HTTP error — the body is extracted and
    // yields an empty list. Both backends must agree on that.
    let w = wasm.get_popular_manga(1, 20, &[]).await.unwrap();
    let y = yaml.get_popular_manga(1, 20, &[]).await.unwrap();
    assert!(w.manga.is_empty());
    assert!(y.manga.is_empty());
}

// ── Group O — the WASM path against a live origin ────────────────────────────

// O2 — the compiled guest builds the request it declared: a search issues a GET
// to /search carrying the query the guest assembled, verified on the wire.
#[tokio::test]
async fn a_wasm_source_builds_the_request_it_declared() {
    let origin = TestOrigin::start().await;
    origin.set("/search", Response::html(SEARCH_HTML));
    let wasm = wasm_or_skip!(origin);

    wasm.search_manga("berserk", 1, 20, &[]).await.unwrap();

    let seen = origin
        .last_request("/search")
        .expect("the guest sent its declared search request");
    assert_eq!(seen.method, "GET", "the guest declared a GET");
    assert_eq!(
        seen.query_param("q").as_deref(),
        Some("berserk"),
        "the search query the guest built reached the wire. Saw: {:?}",
        seen.query
    );
}

// O3 — a preference change reaches a running WASM instance: the fixture reads
// its origin from the `base_url` preference on every call, so updating that
// preference redirects the very next call, no re-instantiation.
#[tokio::test]
async fn a_preference_change_reaches_a_running_wasm_instance() {
    let first = TestOrigin::start().await;
    let second = TestOrigin::start().await;
    first.set("/popular", Response::html(POPULAR_HTML));
    second.set("/popular", Response::html(POPULAR_HTML));

    let wasm = wasm_or_skip!(first);
    wasm.get_popular_manga(1, 20, &[]).await.unwrap();
    assert_eq!(first.hits("/popular"), 1);
    assert_eq!(second.hits("/popular"), 0);

    // Point the running instance at the second origin via a preference update.
    wasm.update_preferences(HashMap::from([("base_url".to_string(), second.base())]));
    wasm.get_popular_manga(1, 20, &[]).await.unwrap();

    assert_eq!(
        second.hits("/popular"),
        1,
        "the preference change reached the running instance — the next call hit the new origin"
    );
}
