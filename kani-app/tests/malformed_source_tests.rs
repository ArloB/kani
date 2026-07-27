#![allow(clippy::unwrap_used)]

//! Group F — malformed and hostile source data against the interpreted
//! `YamlSource` (yaml-only: exercises the evaluator's container/row handling and
//! `unpack_*`). Drives a real popular endpoint against a `TestOrigin`.

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::source::{SourceBackend, YamlSource};
use kani_core::evaluator::EvalLimits;
use kani_shared::ast::Expr;
use kani_shared_test::origin::{Response, TestOrigin};
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedPopular, ValidatedTotalPages,
};
use kani_yaml::yaml::schema::ResponseType;

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

fn popular_endpoint() -> ValidatedEndpoint {
    ValidatedEndpoint {
        route: "/popular".into(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Html,
        container: ".item".into(),
        bindings: vec![],
        fields: vec![attr_field("id", "data-id"), text_field("title", ".title")],
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

/// A YAML source with a `.item`/id+title popular endpoint pointed at `origin`,
/// with optional evaluator-limit override.
fn source(origin: &TestOrigin, limits: EvalLimits) -> SourceBackend {
    let config = ValidatedExtension {
        id: "malformed".into(),
        name: "Malformed".into(),
        version: "1.0.0".into(),
        base_url: origin.base(),
        language: "en".into(),
        unrestricted_http: true,
        popular: Some(ValidatedPopular::Full(Box::new(popular_endpoint()))),
        ..Default::default()
    };
    SourceBackend::Yaml(Box::new(
        YamlSource::new(
            Arc::new(config),
            kani_core::http::SmartClient::new(None).unwrap(),
            Arc::new(kani_core::cache::InMemoryCache::new()),
            "test:".into(),
            HashMap::new(),
            true,
        )
        .with_eval_limits(limits),
    ))
}

fn items(n: usize) -> String {
    let rows: String = (0..n)
        .map(|i| {
            format!(r#"<div class="item" data-id="m{i}"><span class="title">T{i}</span></div>"#)
        })
        .collect();
    format!("<html><body>{rows}</body></html>")
}

// F6 — a listing with more rows than the cap is refused, not silently processed
// or truncated. (Uses the injected EvalLimits so no giant fixture is needed.)
#[tokio::test]
async fn a_listing_past_the_row_cap_is_refused() {
    let origin = TestOrigin::start().await;
    origin.set("/popular", Response::html(&items(12)));
    let backend = source(
        &origin,
        EvalLimits {
            max_list_size: 5,
            ..EvalLimits::default()
        },
    );

    let res = backend.get_popular_manga(1, 50, &[]).await;
    assert!(
        res.is_err(),
        "a 12-row listing must be refused at a cap of 5, got {} rows",
        res.map(|l| l.manga.len()).unwrap_or(0)
    );

    // And a listing within the cap still works.
    origin.set("/popular", Response::html(&items(4)));
    let ok = source(
        &origin,
        EvalLimits {
            max_list_size: 5,
            ..EvalLimits::default()
        },
    );
    assert_eq!(
        ok.get_popular_manga(1, 50, &[]).await.unwrap().manga.len(),
        4
    );
}

// F7 — a required field that the row does not provide is an error, not a silent
// empty/default row.
#[tokio::test]
async fn a_missing_required_field_is_an_error() {
    let origin = TestOrigin::start().await;
    // An item with an id but no `.title` element — `title` is required.
    origin.set(
        "/popular",
        Response::html(r#"<html><body><div class="item" data-id="m1"></div></body></html>"#),
    );
    let backend = source(&origin, EvalLimits::default());

    let res = backend.get_popular_manga(1, 50, &[]).await;
    assert!(
        res.is_err(),
        "a row missing the required title must error, got {:?}",
        res.map(|l| l.manga.len())
    );
}

// F5 — an astral-plane / multibyte id round-trips through extraction unchanged.
#[tokio::test]
async fn an_astral_plane_id_round_trips() {
    let origin = TestOrigin::start().await;
    let id = "漫画-🎉-𝕏";
    origin.set(
        "/popular",
        Response::html(&format!(
            r#"<html><body><div class="item" data-id="{id}"><span class="title">T</span></div></body></html>"#
        )),
    );
    let backend = source(&origin, EvalLimits::default());

    let list = backend.get_popular_manga(1, 50, &[]).await.unwrap();
    assert_eq!(list.manga.len(), 1);
    assert_eq!(list.manga[0].id, id, "the multibyte id survived extraction");
}

// F4 — an enormous field value is refused, not extracted whole. (Driven with a
// small max_string_length via the seam so no multi-MB fixture is needed.)
#[tokio::test]
async fn an_enormous_field_value_is_refused() {
    let origin = TestOrigin::start().await;
    let big = "x".repeat(200);
    origin.set(
        "/popular",
        Response::html(&format!(
            r#"<html><body><div class="item" data-id="m1"><span class="title">{big}</span></div></body></html>"#
        )),
    );
    let backend = source(
        &origin,
        EvalLimits {
            max_string_length: 32,
            ..EvalLimits::default()
        },
    );

    let res = backend.get_popular_manga(1, 50, &[]).await;
    assert!(
        res.is_err(),
        "a 200-char field must be refused at a cap of 32, got {:?}",
        res.map(|l| l.manga.len())
    );
}

// F2 — a row missing the required id is reported, not silently dropped from the
// listing.
#[tokio::test]
async fn a_row_missing_its_id_is_reported_not_dropped() {
    let origin = TestOrigin::start().await;
    // First item has no data-id; a silent-drop bug would return only the second.
    origin.set(
        "/popular",
        Response::html(
            r#"<html><body>
            <div class="item"><span class="title">No Id</span></div>
            <div class="item" data-id="m2"><span class="title">Has Id</span></div>
            </body></html>"#,
        ),
    );
    let backend = source(&origin, EvalLimits::default());

    let res = backend.get_popular_manga(1, 50, &[]).await;
    assert!(
        res.is_err(),
        "a row with a missing id must be reported, not silently dropped (got {:?})",
        res.map(|l| l.manga.len())
    );
}
