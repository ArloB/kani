#![allow(clippy::unwrap_used)]

//! Group B — extension-driven workflows. A rescan that finds *new* chapters is
//! only reachable when the listing grows between scans, which a static fixture
//! can't express. `TestOrigin.script()` serves response N then N+1, so the second
//! scan sees a grown listing.

mod common;
use common::{insert_manga, insert_source, test_service};

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::events::AppEvent;
use kani_app::ids::MangaId;
use kani_app::service::AppService;
use kani_app::source::{SourceBackend, YamlSource};
use kani_shared::ast::Expr;
use kani_shared_test::origin::{Response, TestOrigin};
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedTotalPages,
};
use kani_yaml::yaml::schema::ResponseType;

// ── Listings the origin serves across scans ───────────────────────────────────

const LISTING_2: &str = r#"<html><body>
    <div class="ch" data-id="ch-1"><span class="title">Chapter 1</span></div>
    <div class="ch" data-id="ch-2"><span class="title">Chapter 2</span></div>
</body></html>"#;

const LISTING_3: &str = r#"<html><body>
    <div class="ch" data-id="ch-1"><span class="title">Chapter 1</span></div>
    <div class="ch" data-id="ch-2"><span class="title">Chapter 2</span></div>
    <div class="ch" data-id="ch-3"><span class="title">Chapter 3</span></div>
</body></html>"#;

fn field(name: &str, expr: Expr) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(expr),
        optional: false,
    }
}

fn chapter_list_endpoint() -> ValidatedEndpoint {
    ValidatedEndpoint {
        route: "/manga/$manga_id$/chapters".into(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Html,
        container: ".ch".into(),
        bindings: vec![],
        fields: vec![
            field("id", Expr::self_ref().attr("data-id")),
            field("title", Expr::self_ref().first(".title").text()),
        ],
        scalars: vec![],
        // Static false → the scan makes exactly one request per pass, so the
        // scripted origin advances one listing per scan.
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

/// Register a YAML source (with only a chapter_list endpoint) pointed at `origin`.
async fn wire_source(svc: &AppService, origin: &TestOrigin) -> MangaId {
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga_id = insert_manga(&svc.db, source_id, "m1", "Fixture Manga").await;
    let ext = ValidatedExtension {
        id: "fixture-source".into(),
        name: "Fixture Source".into(),
        version: "1.0.0".into(),
        base_url: origin.base(),
        language: "en".into(),
        unrestricted_http: true,
        chapter_list: Some(chapter_list_endpoint()),
        ..Default::default()
    };
    svc.sources.insert(
        source_id,
        SourceBackend::Yaml(Box::new(YamlSource::new(
            Arc::new(ext),
            kani_core::http::SmartClient::new(None).unwrap(),
            Arc::new(kani_core::cache::InMemoryCache::new()),
            "test:".into(),
            HashMap::new(),
            true,
        ))),
    );
    manga_id
}

/// Drain the refresh channel and count `NewChapters` events (with their counts).
fn drain_new_chapter_counts(rx: &mut tokio::sync::broadcast::Receiver<AppEvent>) -> Vec<usize> {
    let mut counts = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::NewChapters { count, .. } = ev {
            counts.push(count);
        }
    }
    counts
}

// ── B1.1 ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_grown_listing_emits_new_chapters_once_with_the_right_count() {
    let origin = TestOrigin::start().await;
    origin.script(
        "/manga/m1/chapters",
        vec![Response::html(LISTING_2), Response::html(LISTING_3)],
    );
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;

    let first = svc.scan_for_new_chapters(manga_id).await.unwrap();
    assert_eq!(first.len(), 2, "first scan discovers both chapters");

    let mut rx = svc.subscribe_refresh();
    let second = svc.scan_for_new_chapters(manga_id).await.unwrap();
    assert_eq!(second.len(), 1, "only the newly-listed chapter is new");

    assert_eq!(
        drain_new_chapter_counts(&mut rx),
        vec![1],
        "exactly one NewChapters event, count 1 — not 3"
    );
}

// ── B1.2 ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_rescan_with_no_change_emits_nothing() {
    let origin = TestOrigin::start().await;
    origin.script(
        "/manga/m1/chapters",
        vec![Response::html(LISTING_2), Response::html(LISTING_2)],
    );
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;

    svc.scan_for_new_chapters(manga_id).await.unwrap();

    let mut rx = svc.subscribe_refresh();
    let second = svc.scan_for_new_chapters(manga_id).await.unwrap();
    assert!(
        second.is_empty(),
        "an unchanged listing yields no new chapters"
    );
    assert!(
        drain_new_chapter_counts(&mut rx).is_empty(),
        "no NewChapters event fires when nothing is new"
    );
}

// ── B1.4 ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_chapter_that_disappears_from_the_listing_is_not_deleted() {
    // A flaky source drops ch-3 on the second scan; the row must survive.
    let origin = TestOrigin::start().await;
    origin.script(
        "/manga/m1/chapters",
        vec![Response::html(LISTING_3), Response::html(LISTING_2)],
    );
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;

    assert_eq!(svc.scan_for_new_chapters(manga_id).await.unwrap().len(), 3);
    assert!(
        svc.scan_for_new_chapters(manga_id)
            .await
            .unwrap()
            .is_empty()
    );

    let ch3_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chapters WHERE manga_id = ? AND source_chapter_id = 'ch-3'",
    )
    .bind(manga_id.0)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(
        ch3_exists, 1,
        "a chapter that vanished from the source is not deleted"
    );
}

// ── B1.5 ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_relisted_chapter_keeps_its_row_identity() {
    // Re-listing the same chapter ids must not create duplicate rows or change a
    // chapter's row id (its stable identity across scans).
    let origin = TestOrigin::start().await;
    origin.script(
        "/manga/m1/chapters",
        vec![Response::html(LISTING_2), Response::html(LISTING_2)],
    );
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;

    svc.scan_for_new_chapters(manga_id).await.unwrap();
    let id_before: i64 = sqlx::query_scalar(
        "SELECT id FROM chapters WHERE manga_id = ? AND source_chapter_id = 'ch-1'",
    )
    .bind(manga_id.0)
    .fetch_one(&svc.db)
    .await
    .unwrap();

    svc.scan_for_new_chapters(manga_id).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE manga_id = ?")
        .bind(manga_id.0)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    let id_after: i64 = sqlx::query_scalar(
        "SELECT id FROM chapters WHERE manga_id = ? AND source_chapter_id = 'ch-1'",
    )
    .bind(manga_id.0)
    .fetch_one(&svc.db)
    .await
    .unwrap();

    assert_eq!(rows, 2, "no duplicate rows from a re-list");
    assert_eq!(
        id_before, id_after,
        "the chapter keeps its row id across scans"
    );
}

// ── B1.3 ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_new_chapter_fires_the_configured_webhook() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;

    // A wildcard webhook. Delivery to a non-resolving host fails on the network,
    // but the attempt (with its payload) is still recorded — which is what proves
    // the event fired and carried the right data.
    sqlx::query("INSERT INTO webhooks (url, events, enabled) VALUES ('https://hook.invalid/x', '[\"*\"]', 1)")
        .execute(&svc.db)
        .await
        .unwrap();

    svc.spawn_webhook_listener();
    // First discovery of 2 chapters → NewChapters → listener → webhook.
    svc.scan_for_new_chapters(manga_id).await.unwrap();

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let payload = loop {
        let row: Option<String> = sqlx::query_scalar(
            "SELECT payload FROM webhook_deliveries WHERE event_type = 'chapter.new' ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&svc.db)
        .await
        .unwrap();
        if let Some(p) = row {
            break p;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("no chapter.new webhook delivery was recorded");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    assert!(
        payload.contains("Fixture Manga"),
        "payload names the manga: {payload}"
    );
    assert!(
        payload.contains("Chapter 1"),
        "payload names a chapter: {payload}"
    );
}

// ── B2 · auto-download chain (run_auto_scan_once) ─────────────────────────────

/// Count chapter-download jobs submitted (rows persist through terminal status).
async fn download_job_count(svc: &AppService) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE job_type = 'chapter_download'")
        .fetch_one(&svc.db)
        .await
        .unwrap()
}

async fn enable_global_auto_scan(svc: &AppService) {
    svc.settings.write().await.auto_scan = true;
}

// B2.1
#[tokio::test]
async fn a_new_chapter_is_enqueued_when_auto_download_is_on() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;
    enable_global_auto_scan(&svc).await;
    sqlx::query("UPDATE manga SET auto_download = 1 WHERE id = ?")
        .bind(manga_id.0)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.run_auto_scan_once().await;

    assert!(
        download_job_count(&svc).await >= 1,
        "the new chapters are enqueued for download"
    );
}

// B2.2
#[tokio::test]
async fn a_new_chapter_is_not_enqueued_when_auto_download_is_off() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let _manga_id = wire_source(&svc, &origin).await; // auto_download defaults off
    enable_global_auto_scan(&svc).await;

    svc.run_auto_scan_once().await;

    // The chapters are discovered (scanned) but nothing is queued.
    let chapters: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(chapters, 2, "chapters are still scanned in");
    assert_eq!(
        download_job_count(&svc).await,
        0,
        "no download job without auto_download"
    );
}

// B2.3
#[tokio::test]
async fn category_membership_enables_auto_download() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await; // auto_download off on the manga
    enable_global_auto_scan(&svc).await;

    // A category the manga belongs to, marked as an auto-download category.
    let cat_id: i64 =
        sqlx::query_scalar("INSERT INTO categories (name) VALUES ('Follows') RETURNING id")
            .fetch_one(&svc.db)
            .await
            .unwrap();
    sqlx::query("INSERT INTO manga_categories (manga_id, category_id) VALUES (?, ?)")
        .bind(manga_id.0)
        .bind(cat_id)
        .execute(&svc.db)
        .await
        .unwrap();
    svc.settings.write().await.auto_download_category_ids = format!("[{cat_id}]");

    svc.run_auto_scan_once().await;

    assert!(
        download_job_count(&svc).await >= 1,
        "category membership enables auto-download even with the manga flag off"
    );
}

// B2.4
#[tokio::test]
async fn auto_download_respects_download_rules() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;
    enable_global_auto_scan(&svc).await;
    sqlx::query("UPDATE manga SET auto_download = 1 WHERE id = ?")
        .bind(manga_id.0)
        .execute(&svc.db)
        .await
        .unwrap();
    // A rule that admits only a language none of the chapters have — everything
    // is filtered out, so nothing should be enqueued.
    sqlx::query("INSERT INTO download_rules (manga_id, rule_type, value) VALUES (?, 'language_include', 'zz')")
        .bind(manga_id.0)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.run_auto_scan_once().await;

    assert_eq!(
        download_job_count(&svc).await,
        0,
        "chapters filtered out by rules are not enqueued"
    );
}

// B2.6
#[tokio::test]
async fn auto_download_skips_a_manga_with_auto_scan_off() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/m1/chapters", Response::html(LISTING_2));
    let svc = test_service().await;
    let manga_id = wire_source(&svc, &origin).await;
    enable_global_auto_scan(&svc).await;
    sqlx::query("UPDATE manga SET auto_download = 1, auto_scan = 0 WHERE id = ?")
        .bind(manga_id.0)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.run_auto_scan_once().await;

    let chapters: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        chapters, 0,
        "a manga with auto_scan off is not scanned at all"
    );
    assert_eq!(download_job_count(&svc).await, 0, "and nothing is enqueued");
}
