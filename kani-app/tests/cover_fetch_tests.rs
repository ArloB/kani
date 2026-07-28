#![allow(clippy::unwrap_used)]

//! Group K (covers) — the cover download path (`save_to_library` →
//! `download_and_store_cover`). A cover fetch failure is swallowed (the library
//! entry is still saved and a retry scheduled), so the observable is that
//! `local_cover_path` stays NULL — the bad bytes are never stored.

mod common;
use common::{insert_source, test_service};

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::service::AppService;
use kani_app::source::{SourceBackend, YamlSource};
use kani_shared::ast::Expr;
use kani_shared_test::origin::{Response, TestOrigin};
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedTotalPages,
};
use kani_yaml::yaml::schema::ResponseType;

fn lit_field(name: &str, value: Expr) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(value),
        optional: false,
    }
}

/// Register a source whose manga_details returns a fixed cover_url pointing at
/// `{origin}/cover`. No manga row is pre-inserted, so save_to_library inserts it
/// fresh and runs the cover download.
fn wire_cover_source(svc: &AppService, source_id: i64, origin: &TestOrigin) {
    let details = ValidatedEndpoint {
        route: "/manga/$manga_id$".into(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Html,
        container: ".manga".into(),
        bindings: vec![],
        fields: vec![
            lit_field("id", Expr::lit("m1")),
            lit_field("title", Expr::lit("Covered")),
            lit_field("cover_url", Expr::lit(origin.url("/cover"))),
            lit_field("description", Expr::lit("desc")),
            lit_field("status", Expr::lit("ongoing")),
            lit_field("authors", Expr::list(vec![])),
            lit_field("artists", Expr::list(vec![])),
            lit_field("tags", Expr::list(vec![])),
        ],
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
    };
    let ext = ValidatedExtension {
        id: "cover-source".into(),
        name: "Cover Source".into(),
        version: "1.0.0".into(),
        base_url: origin.base(),
        language: "en".into(),
        unrestricted_http: true,
        manga_details: Some(details),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("cover-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

const DETAILS_HTML: &str = r#"<html><body><div class="manga"><h1>x</h1></div></body></html>"#;

async fn cover_path(svc: &AppService, manga: kani_app::ids::MangaId) -> Option<String> {
    sqlx::query_scalar("SELECT local_cover_path FROM manga WHERE id = ?")
        .bind(manga.0)
        .fetch_one(&svc.db)
        .await
        .unwrap()
}

// K7 — a cover served as HTML is rejected by the Content-Type gate and never
// stored: the library entry is saved but local_cover_path stays NULL.
#[tokio::test]
async fn a_cover_served_as_html_is_rejected() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1", Response::html(DETAILS_HTML));
    // The cover URL answers with text/html, not an image.
    origin.set(
        "/cover",
        Response::html("<html>definitely not an image</html>"),
    );

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "cover-source").await;
    wire_cover_source(&svc, source_id, &origin);

    let manga = svc.save_to_library(source_id, "m1", false).await.unwrap();

    assert!(
        cover_path(&svc, manga).await.is_none(),
        "an HTML body must be rejected by the cover Content-Type gate, not stored"
    );
}

// K6 — a cover larger than the 10 MB cap is rejected even with a valid image
// content-type: bytes_limited stops the download, so nothing is stored.
#[tokio::test]
async fn a_cover_larger_than_the_cap_is_rejected() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1", Response::html(DETAILS_HTML));
    // 11 MB, image content-type — passes the type gate, overruns the size cap.
    origin.set("/cover", Response::image(vec![0u8; 11 * 1024 * 1024]));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "cover-source").await;
    wire_cover_source(&svc, source_id, &origin);

    let manga = svc.save_to_library(source_id, "m1", false).await.unwrap();

    assert!(
        cover_path(&svc, manga).await.is_none(),
        "a cover exceeding the 10 MB cap must be rejected, not stored truncated"
    );
}

// K7b — the same gate accepts a real image: the cover is stored.
#[tokio::test]
async fn a_valid_image_cover_is_stored() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1", Response::html(DETAILS_HTML));
    origin.set(
        "/cover",
        Response::image(kani_shared_test::origin::jpeg_page(64, 96, false, 80)),
    );

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "cover-source").await;
    wire_cover_source(&svc, source_id, &origin);

    let manga = svc.save_to_library(source_id, "m1", false).await.unwrap();

    assert!(
        cover_path(&svc, manga).await.is_some(),
        "a genuine image cover passes the gate and is stored"
    );
}
