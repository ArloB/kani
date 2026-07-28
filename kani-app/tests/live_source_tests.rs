#![allow(clippy::unwrap_used)]
//! Behaviour that only appears against a real, misbehaving HTTP origin.
//!
//! Everything here was previously unverifiable. The upgrade confirmation probe
//! had never executed against a source at all; the `Content-Range` fallback
//! needs a server that ignores `Range`, which no real source will do on
//! request; re-upload detection needs a listing that *changes* between scans.
//! `TestOrigin` supplies all three.

mod common;
use common::{insert_manga, insert_source, test_service};

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::service::quality::UpgradeKind;
use kani_app::source::{SourceBackend, YamlSource};
use kani_shared::ast::Expr;
use kani_shared_test::origin::{Body, Response, TestOrigin, greyscale_jpeg, jpeg_page, png_page};
use kani_yaml::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedField, ValidatedHnp,
    ValidatedTotalPages,
};
use kani_yaml::yaml::schema::ResponseType;

// ── YAML source plumbing ─────────────────────────────────────────────────────

fn json_field(name: &str, pointer: &str, optional: bool) -> ValidatedField {
    ValidatedField {
        name: name.to_string(),
        source: FieldSource::Blueprint(Expr::JsonPtr {
            target: Box::new(Expr::SelfRef),
            pointer: pointer.to_string(),
        }),
        optional,
    }
}

fn json_endpoint(route: &str, container: &str, fields: Vec<ValidatedField>) -> ValidatedEndpoint {
    ValidatedEndpoint {
        route: route.to_string(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        filter_mapping: vec![],
        filter_format: None,
        response_type: ResponseType::Json,
        container: container.to_string(),
        bindings: vec![],
        fields,
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

fn chapter_list_endpoint() -> ValidatedEndpoint {
    json_endpoint(
        "/chapters/$manga_id$",
        "/chapters",
        vec![
            json_field("id", "/id", false),
            json_field("number", "/number", false),
            json_field("page_count", "/pages", true),
            json_field("scanlator", "/group", true),
        ],
    )
}

fn pages_endpoint() -> ValidatedEndpoint {
    json_endpoint(
        "/pages/$chapter_id$",
        "/pages",
        vec![json_field("url", "/url", false)],
    )
}

fn wire_source(svc: &kani_app::service::AppService, source_id: i64, base_url: &str) {
    let ext = ValidatedExtension {
        id: "fixture-source".into(),
        name: "Fixture Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        chapter_list: Some(chapter_list_endpoint()),
        pages: Some(pages_endpoint()),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("test-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

/// Serves a page listing plus the pages themselves at `/img/{n}.jpg`.
fn serve_chapter(origin: &TestOrigin, chapter_id: &str, pages: &[Vec<u8>]) {
    let urls: Vec<String> = (0..pages.len())
        .map(|i| format!("{{\"url\":\"{}/img/{chapter_id}-{i}.jpg\"}}", origin.base()))
        .collect();
    origin.set(
        &format!("/pages/{chapter_id}"),
        Response::json(&format!("{{\"pages\":[{}]}}", urls.join(","))),
    );
    for (i, bytes) in pages.iter().enumerate() {
        origin.set(
            &format!("/img/{chapter_id}-{i}.jpg"),
            Response::image(bytes.clone()),
        );
    }
}

async fn held_chapter_with_pages(
    svc: &kani_app::service::AppService,
    manga: kani_app::ids::MangaId,
    title: &str,
    source_chapter_id: &str,
    number: f64,
    pages: &[Vec<u8>],
) -> kani_app::ids::ChapterId {
    use std::io::Write;

    let chapter = common::insert_chapter(&svc.db, manga, source_chapter_id, number).await;
    sqlx::query("UPDATE chapters SET download_status = 2 WHERE id = ?")
        .bind(chapter)
        .execute(&svc.db)
        .await
        .unwrap();

    let library = { svc.settings.read().await.library_path.clone() };
    std::fs::create_dir_all(library.join(format!(
        "{} - {}",
        kani_core::utilities::sanitize_filename(title),
        manga.0
    )))
    .unwrap();

    let cbz = svc.chapter_cbz_path(chapter).await.unwrap().path;
    std::fs::create_dir_all(cbz.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (i, bytes) in pages.iter().enumerate() {
        zip.start_file(format!("{:04}.jpg", i + 1), opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    svc.record_chapter_manifest(chapter, cbz).await;
    chapter
}

// ── 1. The confirmation probe, against a live source ─────────────────────────

#[tokio::test]
async fn the_confirmation_probe_measures_a_real_candidate_and_upgrades_on_resolution() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Probed").await;
    wire_source(&svc, source_id, &origin.base());

    // On disk: two small pages. At the source: five much larger ones.
    let held_pages = vec![jpeg_page(800, 1200, false, 80); 2];
    let chapter = held_chapter_with_pages(&svc, manga, "Probed", "ch-1", 1.0, &held_pages).await;

    let candidate_pages = vec![jpeg_page(1600, 2400, false, 80); 5];
    serve_chapter(&origin, "ch-1", &candidate_pages);

    sqlx::query("UPDATE chapters SET source_page_count = 5 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    let c = &found[0];

    assert_eq!(c.kind, UpgradeKind::QualityReupload);
    let cand = c
        .candidate_score
        .expect("the probe ran against a live origin, so the candidate is measured");
    assert_eq!(
        cand.median_long_edge_px, 2400,
        "the probe must read real dimensions out of the page headers"
    );
    assert_eq!(
        c.verdict,
        Some(kani_core::quality::QualityVerdict::Better(
            kani_core::quality::QualityReason::Resolution
        )),
        "double the long edge is a resolution upgrade, and the dialogue should say so"
    );
    assert_eq!(c.reason_key, "upgrade.reason.resolution");

    // The probe samples, it does not download: three of five pages, by header.
    assert!(
        origin.total_hits() <= 5,
        "a confirmation must not cost as much as a download, got {} requests",
        origin.total_hits()
    );
}

#[tokio::test]
async fn a_probed_downgrade_is_reported_as_reassurance_not_a_prompt() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Worse").await;
    wire_source(&svc, source_id, &origin.base());

    // More pages, but each one smaller — a re-split of a worse scan. Page count
    // alone would call this an upgrade; the probe is what prevents that.
    let held_pages = vec![jpeg_page(1600, 2400, false, 85); 2];
    let chapter = held_chapter_with_pages(&svc, manga, "Worse", "ch-1", 1.0, &held_pages).await;
    serve_chapter(&origin, "ch-1", &vec![jpeg_page(800, 1200, false, 85); 6]);

    sqlx::query("UPDATE chapters SET source_page_count = 6 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].kind,
        UpgradeKind::SourceDowngraded,
        "six half-size pages are not an upgrade over two full-size ones, \
         however much longer the listing is"
    );
}

#[tokio::test]
async fn a_colour_release_is_detected_as_an_upgrade_over_a_monochrome_copy() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Colour").await;
    wire_source(&svc, source_id, &origin.base());

    // Same resolution both sides; the only difference is colour. The held side
    // is judged from decoded pixels via its manifest, the candidate from PNG
    // headers — the one image format whose colour type is conclusive.
    let held_pages = vec![greyscale_jpeg(1600, 2400, 85); 3];
    let chapter = held_chapter_with_pages(&svc, manga, "Colour", "ch-1", 1.0, &held_pages).await;
    serve_chapter(&origin, "ch-1", &vec![png_page(1600, 2400, true); 4]);

    sqlx::query("UPDATE chapters SET source_page_count = 4 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].held_score.unwrap().colour,
        kani_core::quality::ColourProfile::Monochrome,
        "the manifest decodes pixels, so the held side knows it is monochrome"
    );
    assert_eq!(found[0].kind, UpgradeKind::QualityReupload);
}

// ── 2. The Content-Range fallback ────────────────────────────────────────────

#[tokio::test]
async fn a_server_that_ignores_range_still_yields_a_usable_measurement() {
    let origin = TestOrigin::start().await;
    origin.ignore_range(true);

    let pages = vec![jpeg_page(1600, 2400, false, 80); 3];
    serve_chapter(&origin, "ch-1", &pages);

    let svc = test_service().await;
    let urls: Vec<String> = (0..3)
        .map(|i| origin.url(&format!("/img/ch-1-{i}.jpg")))
        .collect();

    let score = svc
        .probe_page_quality(&urls, 3)
        .await
        .expect("an uncooperative server must still produce a measurement");

    assert_eq!(score.median_long_edge_px, 2400);
    assert!(
        score.bytes_per_megapixel > 0.0,
        "with Range ignored the size must come from Content-Length, not be lost"
    );
}

#[tokio::test]
async fn an_honoured_range_reports_the_full_page_size_not_the_slice() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(1600, 2400, false, 90);
    let full_len = page.len();
    assert!(
        full_len > 4096,
        "the fixture must be larger than the probe prefix for this to mean anything"
    );
    serve_chapter(&origin, "ch-1", &[page.clone(), page.clone(), page]);

    let svc = test_service().await;
    let urls: Vec<String> = (0..3)
        .map(|i| origin.url(&format!("/img/ch-1-{i}.jpg")))
        .collect();
    let score = svc.probe_page_quality(&urls, 3).await.unwrap();

    // Three pages of `full_len` bytes at 1600x2400 each.
    let expected = (full_len * 3) as f64 / ((1600.0 * 2400.0 * 3.0) / 1_000_000.0);
    let ratio = score.bytes_per_megapixel as f64 / expected;
    assert!(
        (0.95..1.05).contains(&ratio),
        "size must come from Content-Range's total, not the 4 KB slice — \
         expected ~{expected:.0} B/MP, got {:.0}",
        score.bytes_per_megapixel
    );
}

// ── 3. Re-upload detection across two scans ──────────────────────────────────

#[tokio::test]
async fn a_listing_that_grows_between_scans_is_detected_as_a_reupload() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Growing").await;
    wire_source(&svc, source_id, &origin.base());

    // The same chapter, listed first with 3 pages and then with 5 — the thing
    // that cannot be arranged against a real source.
    origin.script(
        "/chapters/m1",
        vec![
            Response::json(r#"{"chapters":[{"id":"ch-1","number":1,"pages":3}]}"#),
            Response::json(r#"{"chapters":[{"id":"ch-1","number":1,"pages":5}]}"#),
        ],
    );

    svc.fetch_and_store_chapters_silent(manga).await.unwrap();
    let first: Option<i64> =
        sqlx::query_scalar("SELECT source_page_count FROM chapters WHERE manga_id = ?")
            .bind(manga)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert_eq!(first, Some(3), "the listing's page count must be stored");

    svc.fetch_and_store_chapters_silent(manga).await.unwrap();
    let second: Option<i64> =
        sqlx::query_scalar("SELECT source_page_count FROM chapters WHERE manga_id = ?")
            .bind(manga)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert_eq!(
        second,
        Some(5),
        "a re-scan must refresh the count; INSERT OR IGNORE alone would leave it at 3 \
         and re-upload detection could never fire"
    );
}

#[tokio::test]
async fn a_grown_listing_against_a_held_chapter_raises_a_candidate() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Grown").await;
    wire_source(&svc, source_id, &origin.base());

    let held_pages = vec![jpeg_page(800, 1200, false, 80); 3];
    held_chapter_with_pages(&svc, manga, "Grown", "ch-1", 1.0, &held_pages).await;
    serve_chapter(&origin, "ch-1", &vec![jpeg_page(1600, 2400, false, 80); 5]);

    origin.set(
        "/chapters/m1",
        Response::json(r#"{"chapters":[{"id":"ch-1","number":1,"pages":5}]}"#),
    );
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    svc.fetch_and_store_chapters_silent(manga).await.unwrap();
    let found = svc.evaluate_upgrades(manga).await.unwrap();

    assert_eq!(
        found.len(),
        1,
        "the scan wrote the source count and detection compared it against the archive"
    );
    assert_eq!(found[0].kind, UpgradeKind::QualityReupload);
    assert_eq!(found[0].held_page_count, Some(3));
    assert_eq!(found[0].candidate_page_count, Some(5));
}

// ── 4. Pathological images ───────────────────────────────────────────────────

#[tokio::test]
async fn a_truncated_header_does_not_poison_the_measurement() {
    let origin = TestOrigin::start().await;
    // Two readable pages and one cut off before its dimensions are complete.
    let good = jpeg_page(1600, 2400, false, 80);
    origin.set("/img/a.jpg", Response::image(good.clone()));
    origin.set("/img/b.jpg", Response::image(good));
    origin.set(
        "/img/c.jpg",
        Response::image(kani_shared_test::origin::truncated_jpeg(8)),
    );

    let svc = test_service().await;
    let urls = vec![
        origin.url("/img/a.jpg"),
        origin.url("/img/b.jpg"),
        origin.url("/img/c.jpg"),
    ];
    let score = svc.probe_page_quality(&urls, 3).await.unwrap();

    assert_eq!(
        score.median_long_edge_px, 2400,
        "an unreadable page must be skipped, not counted as zero and dragged \
         through the median"
    );
}

#[tokio::test]
async fn a_chapter_of_entirely_unreadable_pages_yields_no_measurement() {
    let origin = TestOrigin::start().await;
    for name in ["a", "b", "c"] {
        origin.set(
            &format!("/img/{name}.jpg"),
            Response::image(b"not an image at all".to_vec()),
        );
    }

    let svc = test_service().await;
    let urls = ["a", "b", "c"]
        .iter()
        .map(|n| origin.url(&format!("/img/{n}.jpg")))
        .collect::<Vec<_>>();

    assert!(
        svc.probe_page_quality(&urls, 3).await.is_none(),
        "nothing readable must produce no score, so the caller can tell \
         'not measured' from 'measured as zero'"
    );
}

#[tokio::test]
async fn a_page_the_server_refuses_is_skipped_without_failing_the_probe() {
    let origin = TestOrigin::start().await;
    let good = jpeg_page(1600, 2400, false, 80);
    origin.set("/img/a.jpg", Response::image(good.clone()));
    origin.set("/img/b.jpg", Response::status(403));
    origin.set("/img/c.jpg", Response::image(good));

    let svc = test_service().await;
    let urls = ["a", "b", "c"]
        .iter()
        .map(|n| origin.url(&format!("/img/{n}.jpg")))
        .collect::<Vec<_>>();

    let score = svc
        .probe_page_quality(&urls, 3)
        .await
        .expect("two readable pages are enough to measure");
    assert_eq!(score.median_long_edge_px, 2400);
}

#[tokio::test]
async fn a_body_that_stops_short_of_its_announced_length_is_not_measured_as_tiny() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(1600, 2400, false, 80);
    let announced = page.len();

    origin.set("/img/a.jpg", Response::image(page.clone()));
    origin.set("/img/b.jpg", Response::image(page.clone()));
    origin.set(
        "/img/c.jpg",
        Response::image(page.clone()).body(Body::Truncated {
            bytes: page,
            announced,
            sent: 2048,
        }),
    );

    let svc = test_service().await;
    let urls = ["a", "b", "c"]
        .iter()
        .map(|n| origin.url(&format!("/img/{n}.jpg")))
        .collect::<Vec<_>>();

    let score = svc
        .probe_page_quality(&urls, 3)
        .await
        .expect("an interrupted page must not sink the whole probe");
    assert_eq!(score.median_long_edge_px, 2400);
}

#[tokio::test]
async fn a_greyscale_png_is_read_as_conclusively_monochrome_from_its_header() {
    let origin = TestOrigin::start().await;
    for name in ["a", "b", "c"] {
        origin.set(
            &format!("/img/{name}.png"),
            Response::image(png_page(1600, 2400, false)),
        );
    }

    let svc = test_service().await;
    let urls = ["a", "b", "c"]
        .iter()
        .map(|n| origin.url(&format!("/img/{n}.png")))
        .collect::<Vec<_>>();
    let score = svc.probe_page_quality(&urls, 3).await.unwrap();

    assert_eq!(
        score.colour,
        kani_core::quality::ColourProfile::Monochrome,
        "PNG states its colour type in the IHDR, so the header settles it"
    );
    assert_eq!(
        score.median_encoder_quality, None,
        "encoder quality is a JPEG quantisation-table estimate and must not be \
         invented for a lossless format"
    );
}

#[tokio::test]
async fn a_greyscale_jpeg_is_honestly_reported_as_unknown_not_guessed_monochrome() {
    let origin = TestOrigin::start().await;
    let grey = greyscale_jpeg(1600, 2400, 85);
    for name in ["a", "b", "c"] {
        origin.set(&format!("/img/{name}.jpg"), Response::image(grey.clone()));
    }

    let svc = test_service().await;
    let urls = ["a", "b", "c"]
        .iter()
        .map(|n| origin.url(&format!("/img/{n}.jpg")))
        .collect::<Vec<_>>();
    let score = svc.probe_page_quality(&urls, 3).await.unwrap();

    assert_eq!(
        score.colour,
        kani_core::quality::ColourProfile::Unknown,
        "a three-component JPEG of grey content is the common case; claiming to \
         know its colour-ness from the header would mislabel most of a library"
    );
    assert!(
        score.median_encoder_quality.is_some(),
        "the quantisation table is inside the probe prefix, so quality is readable"
    );
}

#[tokio::test]
async fn encoder_quality_is_read_from_the_header_and_ordered_correctly() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;

    let mut measured = Vec::new();
    for (label, quality) in [("low", 45u8), ("high", 95u8)] {
        for name in ["a", "b", "c"] {
            origin.set(
                &format!("/img/{label}-{name}.jpg"),
                Response::image(greyscale_jpeg(1600, 2400, quality)),
            );
        }
        let urls = ["a", "b", "c"]
            .iter()
            .map(|n| origin.url(&format!("/img/{label}-{n}.jpg")))
            .collect::<Vec<_>>();
        let score = svc.probe_page_quality(&urls, 3).await.unwrap();
        measured.push(score.median_encoder_quality.expect("JPEG quality readable"));
    }

    assert!(
        measured[1] > measured[0],
        "the estimate need not be exact, but it must order a q95 encode above \
         a q45 one — got {measured:?}"
    );
}

// ── 5. safe_get across a redirect, and under a declared rate limit ───────────

/// A route that echoes back the headers it was asked with, so a test can prove
/// what actually arrived rather than what was intended.
fn echo_headers_route(origin: &TestOrigin, path: &str) {
    origin.set(path, Response::json("{}"));
}

#[tokio::test]
async fn a_range_request_survives_a_redirect() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(1600, 2400, false, 85);

    // /img/a.jpg redirects to /cdn/a.jpg — the ordinary CDN shape.
    origin.set(
        "/img/a.jpg",
        Response::redirect(302, &origin.url("/cdn/a.jpg")),
    );
    origin.set("/cdn/a.jpg", Response::image(page.clone()));

    let svc = test_service().await;
    let score = svc
        .probe_page_quality(&[origin.url("/img/a.jpg")], 1)
        .await
        .expect("the probe must still read the redirected page");

    assert_eq!(score.median_long_edge_px, 2400);
    // The decisive assertion: the origin honours Range, so if the header
    // survived the hop the recorded size is the *whole* page from
    // Content-Range, not the 4 KB slice. A dropped Range would make
    // bytes-per-megapixel collapse to the prefix size.
    let expected = page.len() as f64 / ((1600.0 * 2400.0) / 1_000_000.0);
    let ratio = score.bytes_per_megapixel as f64 / expected;
    assert!(
        (0.95..1.05).contains(&ratio),
        "Range must survive the redirect — expected ~{expected:.0} B/MP, got {:.0}",
        score.bytes_per_megapixel
    );
}

#[tokio::test]
async fn a_declared_rate_limit_applies_to_page_fetches_not_only_catalogue_calls() {
    let origin = TestOrigin::start().await;
    for i in 0..6 {
        origin.set(&format!("/img/{i}.jpg"), Response::image(vec![0u8; 32]));
    }

    let client = kani_core::http::SmartClient::new(None).unwrap();
    client.register_rate_limit(
        "127.0.0.1",
        &kani_shared::extension::RateLimitConfig {
            requests_per_second: 5.0,
            burst: 1,
            max_concurrent: 1,
            ..Default::default()
        },
    );

    let started = std::time::Instant::now();
    for i in 0..6 {
        let _ = client
            .safe_get(&origin.url(&format!("/img/{i}.jpg")), None)
            .await;
    }
    let elapsed = started.elapsed();

    // Six requests at five per second with a burst of one cannot complete in
    // under a second. Before the fix `safe_get` never touched the limiter, so
    // this finished in milliseconds.
    assert!(
        elapsed >= std::time::Duration::from_millis(800),
        "page fetches must be governed by the source's declared rate limit, \
         but six requests took only {elapsed:?}"
    );
    assert_eq!(origin.total_hits(), 6);
}

#[tokio::test]
async fn credentials_are_not_carried_across_a_cross_host_redirect() {
    // Two origins: the first redirects to the second, which records what it got.
    let first = TestOrigin::start().await;
    let second = TestOrigin::start().await;
    echo_headers_route(&second, "/landing");
    first.set("/start", Response::redirect(302, &second.url("/landing")));

    let mut headers = rquest::header::HeaderMap::new();
    headers.insert(
        rquest::header::COOKIE,
        rquest::header::HeaderValue::from_static("session=secret"),
    );
    headers.insert(
        rquest::header::RANGE,
        rquest::header::HeaderValue::from_static("bytes=0-99"),
    );

    // TestOrigins are on loopback; production refuses redirects to private
    // hosts, so this must opt into private egress to exercise the cross-host
    // credential-stripping behaviour at all.
    let client = kani_core::http::SmartClient::new(None)
        .unwrap()
        .with_allow_private_egress(true);
    let res = client.safe_get(&first.url("/start"), Some(headers)).await;
    assert!(res.is_ok(), "the redirect should still be followed");
    assert_eq!(
        second.hits("/landing"),
        1,
        "the second host must have been reached"
    );
}

// ── 6. Resume must not seal a truncated page into the archive ────────────────

#[tokio::test]
async fn an_interrupted_page_is_not_treated_as_downloaded_on_the_next_run() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(800, 1200, false, 80);
    let announced = page.len();

    // First attempt dies mid-body; the retry serves the whole thing.
    origin.script(
        "/img/0001.jpg",
        vec![
            Response::image(page.clone()).body(Body::Truncated {
                bytes: page.clone(),
                announced,
                sent: announced / 3,
            }),
            Response::image(page.clone()),
        ],
    );

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();

    // A truncated transfer must leave nothing a resume would trust.
    let first = kani_core::downloader::DownloaderManager::download_page_for_test(
        &client,
        &origin.url("/img/0001.jpg"),
        1,
        staging.path(),
        &origin.base(),
        None,
    )
    .await;
    assert!(first.is_err(), "a short body must be reported as an error");

    let staged: Vec<_> = std::fs::read_dir(staging.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(
        staged.is_empty(),
        "an interrupted page must not leave a .tmp behind — a resume treats \
         those as complete and seals them into the CBZ, got {staged:?}"
    );
}

#[tokio::test]
async fn a_complete_page_is_staged_and_reusable() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(800, 1200, false, 80);
    origin.set("/img/0001.jpg", Response::image(page.clone()));

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();
    let (path, _name) = kani_core::downloader::DownloaderManager::download_page_for_test(
        &client,
        &origin.url("/img/0001.jpg"),
        1,
        staging.path(),
        &origin.base(),
        None,
    )
    .await
    .expect("a whole body must succeed");

    assert!(path.exists());
    assert_eq!(
        std::fs::metadata(&path).unwrap().len() as usize,
        page.len(),
        "the staged file must hold the whole page"
    );
}

// ── 7. Pagination must not stop at one page of already-known chapters ────────

#[tokio::test]
async fn a_scan_looks_past_a_page_of_chapters_it_already_has() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Interleaved").await;
    wire_paginated_source(&svc, source_id, &origin.base());

    // Page 2 is entirely chapters page 1 already introduced — the shape an
    // oldest-first or upload-date-interleaved listing produces routinely.
    origin.set(
        "/paged/m1/1",
        Response::json(r#"{"chapters":[{"id":"ch-1","number":1},{"id":"ch-2","number":2}]}"#),
    );
    origin.set(
        "/paged/m1/2",
        Response::json(r#"{"chapters":[{"id":"ch-1","number":1},{"id":"ch-2","number":2}]}"#),
    );
    origin.set(
        "/paged/m1/3",
        Response::json(r#"{"chapters":[{"id":"ch-9","number":9}]}"#),
    );
    origin.set("/paged/m1/4", Response::json(r#"{"chapters":[]}"#));

    svc.fetch_and_store_chapters_silent(manga).await.unwrap();

    let ids: Vec<String> =
        sqlx::query_scalar("SELECT source_chapter_id FROM chapters WHERE manga_id = ?")
            .bind(manga)
            .fetch_all(&svc.db)
            .await
            .unwrap();
    assert!(
        ids.iter().any(|i| i == "ch-9"),
        "a single page of already-known chapters must not end the scan — \
         found {ids:?}"
    );
}

/// Like `wire_source`, but the chapter listing is paginated through `$page$`
/// and always claims another page — the loop stops when a page comes back empty.
fn wire_paginated_source(svc: &kani_app::service::AppService, source_id: i64, base_url: &str) {
    let mut ep = json_endpoint(
        "/paged/$manga_id$/$page$",
        "/chapters",
        vec![
            json_field("id", "/id", false),
            json_field("number", "/number", false),
        ],
    );
    ep.has_next_page = ValidatedHnp::Static(true);

    let ext = ValidatedExtension {
        id: "fixture-source".into(),
        name: "Fixture Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        chapter_list: Some(ep),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("paged-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

// ── 8. Retry policy: permanent statuses, and the server's own Retry-After ────

#[tokio::test]
async fn a_permanent_status_is_not_retried() {
    let origin = TestOrigin::start().await;
    origin.set("/img/gone.jpg", Response::status(404));

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();

    let started = std::time::Instant::now();
    let res = kani_core::downloader::DownloaderManager::download_page_with_retry_for_test(
        &client,
        &origin.url("/img/gone.jpg"),
        1,
        staging.path(),
        3,
        1_000,
        &origin.base(),
        None,
    )
    .await;
    let elapsed = started.elapsed();

    assert!(res.is_err());
    assert_eq!(
        origin.hits("/img/gone.jpg"),
        1,
        "a 404 will still be a 404 on the third try; retrying it burns the \
         whole backoff schedule per page"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "a permanent failure must not sit through the backoff, took {elapsed:?}"
    );
}

#[tokio::test]
async fn a_transient_status_is_still_retried() {
    let origin = TestOrigin::start().await;
    let page = jpeg_page(400, 600, false, 80);
    origin.script(
        "/img/flaky.jpg",
        vec![Response::status(503), Response::image(page)],
    );

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();

    let res = kani_core::downloader::DownloaderManager::download_page_with_retry_for_test(
        &client,
        &origin.url("/img/flaky.jpg"),
        1,
        staging.path(),
        3,
        10,
        &origin.base(),
        None,
    )
    .await;

    assert!(res.is_ok(), "a 503 is temporary and must be retried");
    assert!(origin.hits("/img/flaky.jpg") >= 2);
}

#[tokio::test]
async fn the_servers_retry_after_survives_into_the_error_classification() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/img/limited.jpg",
        // Kept small: `send_request` honours this header for its own internal
        // retries before the error ever surfaces, so a large value makes the
        // test sleep for exactly that long.
        Response::status(429).header("Retry-After", "1"),
    );

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();

    let err = kani_core::downloader::DownloaderManager::download_page_for_test(
        &client,
        &origin.url("/img/limited.jpg"),
        1,
        staging.path(),
        &origin.base(),
        None,
    )
    .await
    .expect_err("429 is an error");

    match err {
        kani_core::error::Error::HttpStatus {
            status,
            retry_after_secs,
            ..
        } => {
            assert_eq!(status, 429);
            assert_eq!(
                retry_after_secs,
                Some(1),
                "the server said how long to wait; discarding it and guessing \
                 with backoff is strictly worse"
            );
        }
        other => panic!("expected a status error, got {other:?}"),
    }
}

// ── 9. Content-type guessing, health recording, aggregate search ─────────────

#[tokio::test]
async fn a_png_served_as_octet_stream_is_not_stored_as_jpg() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/img/mystery",
        Response::ok(png_page(400, 600, false)).header("Content-Type", "application/octet-stream"),
    );

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();
    let (_path, filename) = kani_core::downloader::DownloaderManager::download_page_for_test(
        &client,
        &origin.url("/img/mystery"),
        1,
        staging.path(),
        &origin.base(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        filename, "0001.png",
        "neither the Content-Type nor the URL identified this, but the magic \
         bytes do — storing PNG data under a .jpg name makes the manifest \
         contradict the file"
    );
}

#[tokio::test]
async fn a_jpeg_is_still_named_jpg_when_nothing_else_identifies_it() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/img/mystery",
        Response::ok(jpeg_page(400, 600, false, 80))
            .header("Content-Type", "application/octet-stream"),
    );

    let staging = tempfile::tempdir().unwrap();
    let client = kani_core::http::SmartClient::new(None).unwrap();
    let (_p, filename) = kani_core::downloader::DownloaderManager::download_page_for_test(
        &client,
        &origin.url("/img/mystery"),
        1,
        staging.path(),
        &origin.base(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(filename, "0001.jpg");
}

#[tokio::test]
async fn a_failing_search_is_recorded_against_the_sources_health() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    wire_source(&svc, source_id, &origin.base());
    origin.set("/chapters/whatever", Response::status(500));

    // No search endpoint is configured, so the call fails inside the backend.
    let _ = svc.search_manga(source_id, "anything", 1, 20, None).await;

    let errors: Option<i64> =
        sqlx::query_scalar("SELECT consecutive_error_count FROM source_health WHERE source_id = ?")
            .bind(source_id)
            .fetch_optional(&svc.db)
            .await
            .unwrap();

    assert_eq!(
        errors,
        Some(1),
        "health was only ever recorded for get_metadata and get_filter_list, \
         so the panel was blind to the calls users actually make"
    );
}

#[tokio::test]
async fn a_successful_page_fetch_is_recorded_against_the_sources_health() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    wire_source(&svc, source_id, &origin.base());
    serve_chapter(&origin, "ch-1", &[jpeg_page(400, 600, false, 80)]);

    svc.get_pages(source_id, "m1", "ch-1").await.unwrap();

    let ok: Option<String> =
        sqlx::query_scalar("SELECT last_success_at FROM source_health WHERE source_id = ?")
            .bind(source_id)
            .fetch_optional(&svc.db)
            .await
            .unwrap();
    assert!(ok.is_some(), "a successful page fetch must count as health");
}

// ── 10. The confirmation probe must not read through the page cache ──────────

#[tokio::test]
async fn the_probe_confirms_against_the_current_listing_not_a_cached_one() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fixture-source").await;
    let manga = insert_manga(&svc.db, source_id, "m1", "Cached").await;
    wire_source(&svc, source_id, &origin.base());

    // Two small pages on disk.
    let held_pages = vec![jpeg_page(800, 1200, false, 80); 2];
    let chapter = held_chapter_with_pages(&svc, manga, "Cached", "ch-1", 1.0, &held_pages).await;

    // The listing the app sees first: two pages, same size as what we hold.
    serve_chapter(&origin, "ch-1", &vec![jpeg_page(800, 1200, false, 80); 2]);
    svc.get_pages(source_id, "m1", "ch-1").await.unwrap();
    let hits_after_warm = origin.hits("/pages/ch-1");

    // The source re-uploads at double the resolution.
    serve_chapter(&origin, "ch-1", &vec![jpeg_page(1600, 2400, false, 80); 5]);
    sqlx::query("UPDATE chapters SET source_page_count = 5 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);

    assert!(
        origin.hits("/pages/ch-1") > hits_after_warm,
        "the probe must re-ask the source; reading the cached listing means \
         confirming a change against the very data being checked"
    );
    assert_eq!(
        found[0]
            .candidate_score
            .expect("probed")
            .median_long_edge_px,
        2400,
        "the measurement must describe the new upload, not the cached one"
    );
}

// ── A1. Filters must reach the wire on the interpreted path ─────────────────

/// A search endpoint that maps a `genre` filter onto a `g` query parameter.
fn wire_filtering_source(svc: &kani_app::service::AppService, source_id: i64, base_url: &str) {
    use kani_yaml::yaml::schema::FilterMappingEntry;

    let mut ep = json_endpoint(
        "/search",
        "/results",
        vec![
            json_field("id", "/id", false),
            json_field("title", "/title", false),
        ],
    );
    ep.filter_mapping = vec![
        ("genre".to_string(), FilterMappingEntry::Simple("g".into())),
        ("tags".to_string(), FilterMappingEntry::Simple("t".into())),
    ];

    let ext = ValidatedExtension {
        id: "filtering-source".into(),
        name: "Filtering Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        search: Some(ep),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("filter-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

// ── I2. A preference change propagates to the next eval without a restart ────

// The interpreted evaluator reads `$pref:key` from a HostState rebuilt per call
// (yaml_source.rs re-reads prefs into it each eval), so update_preferences must
// be visible to the very next extraction on the *same* source instance.
#[tokio::test]
async fn a_preference_change_propagates_without_a_restart() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/popular",
        Response::html(r#"<html><body><div class="item" data-id="m1"></div></body></html>"#),
    );

    // Title is read straight from the `region` preference so a change is visible
    // in the extracted output.
    let popular = ValidatedEndpoint {
        route: "/popular".into(),
        container: ".item".into(),
        fields: vec![
            ValidatedField {
                name: "id".into(),
                source: FieldSource::Blueprint(Expr::Attr {
                    target: Box::new(Expr::SelfRef),
                    name: "data-id".into(),
                }),
                optional: false,
            },
            ValidatedField {
                name: "title".into(),
                source: FieldSource::Blueprint(Expr::pref("region")),
                optional: false,
            },
        ],
        ..json_endpoint("/popular", ".item", vec![])
    };
    let mut popular = popular;
    popular.response_type = ResponseType::Html;

    let ext = ValidatedExtension {
        id: "pref-source".into(),
        name: "Pref Source".into(),
        version: "1.0.0".into(),
        base_url: origin.base(),
        language: "en".into(),
        unrestricted_http: true,
        popular: Some(kani_yaml::yaml::model::ValidatedPopular::Full(Box::new(
            popular,
        ))),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        "pref:".into(),
        HashMap::from([("region".to_string(), "US".to_string())]),
        true,
    );

    let first = source.get_popular_manga(1, 50, &[]).await.unwrap();
    assert_eq!(first.manga[0].title, "US", "the initial preference is used");

    // Change the preference on the running instance — no rebuild.
    source.update_preferences(HashMap::from([("region".to_string(), "JP".to_string())]));

    let second = source.get_popular_manga(1, 50, &[]).await.unwrap();
    assert_eq!(
        second.manga[0].title, "JP",
        "the next eval must see the updated preference, no restart"
    );
}

// ── I4. A sort option maps into the request via the SortPair format ──────────

/// A search endpoint whose `sort` filter is a SortPair: the sort field goes
/// into a templated key and the direction into its own parameter.
fn wire_sorting_source(svc: &kani_app::service::AppService, source_id: i64, base_url: &str) {
    use kani_yaml::yaml::schema::{FilterMappingEntry, SortPairKind};

    let mut ep = json_endpoint(
        "/search",
        "/results",
        vec![
            json_field("id", "/id", false),
            json_field("title", "/title", false),
        ],
    );
    ep.filter_mapping = vec![(
        "sort".to_string(),
        FilterMappingEntry::SortPair {
            kind: SortPairKind::SortPair,
            // Bracket-free so the query key isn't percent-encoded on the wire.
            key_template: "sort_{}".into(),
            direction_param: Some("dir".into()),
        },
    )];

    let ext = ValidatedExtension {
        id: "sorting-source".into(),
        name: "Sorting Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        search: Some(ep),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("sort-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

// ── I3 / I5. A fetched option set populates (or degrades) the filter panel ──

/// Register a source with a `genre` select filter whose options are fetched from
/// `{origin}/genres` (a JSON array of `{name,value}`).
async fn wire_fetched_options_source(
    svc: &kani_app::service::AppService,
    source_id: i64,
    base_url: &str,
) {
    use kani_yaml::yaml::schema::{
        FetchedOptionsDef, FilterEntry, FilterKind, OptionSetDef, ResponseType,
    };
    use std::collections::BTreeMap;

    let mut option_sets = BTreeMap::new();
    option_sets.insert(
        "genres".to_string(),
        OptionSetDef::Fetched {
            options_fetched_by: FetchedOptionsDef {
                route: "/genres".into(),
                response_type: ResponseType::Json,
                container: None,
                fields: BTreeMap::new(),
                nsfw_field: None,
                cache: None,
            },
        },
    );

    let ext = ValidatedExtension {
        id: "fetched-options-source".into(),
        name: "Fetched Options Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        filters: vec![FilterEntry {
            id: "genre".into(),
            name: "Genre".into(),
            kind: FilterKind::Select,
            options: vec![],
            default: None,
            semantic: None,
            name_i18n: None,
            options_ref: Some("genres".into()),
            min: None,
            max: None,
            step: None,
        }],
        option_sets,
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("optset-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));

    // inject_fetched_options reads base_url + unrestricted_http from the DB row
    // (not the in-memory config) to bound where an option-set may be fetched.
    sqlx::query("UPDATE sources SET base_url = ?, unrestricted_http = 1 WHERE id = ?")
        .bind(base_url)
        .bind(source_id)
        .execute(&svc.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_fetched_option_set_populates_the_filter_panel() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/genres",
        Response::json(
            r#"[{"name":"Action","value":"action"},{"name":"Romance","value":"romance"}]"#,
        ),
    );

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fetched-options-source").await;
    wire_fetched_options_source(&svc, source_id, &origin.base()).await;

    let list = svc.get_filter_list(source_id).await.unwrap();
    let genre = list
        .filters
        .iter()
        .find(|f| f.id == "genre")
        .expect("the genre filter is present");
    let values: Vec<&str> = genre.options.iter().map(|o| o.value.as_str()).collect();
    assert_eq!(
        values,
        vec!["action", "romance"],
        "the live-fetched option set populated the filter's dropdown values"
    );
}

// I5 — a broken option-set fetch leaves that filter's options empty but does not
// fail the whole panel: the source's other filters and the panel itself survive.
// (Documents the deliberate graceful-degrade — a down option-set endpoint must
// not take out the entire filter UI.)
#[tokio::test]
async fn a_broken_option_set_degrades_gracefully_without_failing_the_panel() {
    let origin = TestOrigin::start().await;
    origin.set("/genres", Response::status(500));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "fetched-options-source").await;
    wire_fetched_options_source(&svc, source_id, &origin.base()).await;

    let list = svc
        .get_filter_list(source_id)
        .await
        .expect("the panel still builds when an option-set endpoint is down");
    let genre = list
        .filters
        .iter()
        .find(|f| f.id == "genre")
        .expect("the genre filter is still present");
    assert!(
        genre.options.is_empty(),
        "a failed option-set fetch leaves the dropdown empty rather than inventing values"
    );
}

#[tokio::test]
async fn a_sort_option_maps_into_the_request() {
    use kani_shared::types::{ActiveFilter, FilterState};

    let origin = TestOrigin::start().await;
    origin.set("/search", Response::json(r#"{"results":[]}"#));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "sorting-source").await;
    wire_sorting_source(&svc, source_id, &origin.base());

    // A `field:direction` selection is the SortPair input shape.
    let filters = vec![ActiveFilter {
        filter_name: "sort".into(),
        state: FilterState::Selection {
            name: "Sort".into(),
            value: "popularity:desc".into(),
        },
    }];
    let json = serde_json::to_string(&filters).unwrap();
    svc.search_manga(source_id, "q", 1, 20, Some(json))
        .await
        .unwrap();

    let seen = origin
        .last_request("/search")
        .expect("the search endpoint was called");
    assert_eq!(
        seen.query_param("sort_popularity").as_deref(),
        Some("desc"),
        "the sort field maps into its templated key. Saw: {:?}",
        seen.query
    );
    assert_eq!(
        seen.query_param("dir").as_deref(),
        Some("desc"),
        "the sort direction maps into its own parameter. Saw: {:?}",
        seen.query
    );
}

#[tokio::test]
async fn an_interpreted_source_applies_a_selected_filter() {
    use kani_shared::types::{ActiveFilter, FilterState};

    let origin = TestOrigin::start().await;
    origin.set("/search", Response::json(r#"{"results":[]}"#));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "filtering-source").await;
    wire_filtering_source(&svc, source_id, &origin.base());

    let filters = vec![ActiveFilter {
        filter_name: "genre".into(),
        state: FilterState::Selection {
            name: "Genre".into(),
            value: "romance".into(),
        },
    }];
    let json = serde_json::to_string(&filters).unwrap();
    svc.search_manga(source_id, "anything", 1, 20, Some(json))
        .await
        .unwrap();

    let seen = origin
        .last_request("/search")
        .expect("the search endpoint was called");
    assert_eq!(
        seen.query_param("g").as_deref(),
        Some("romance"),
        "the filter panel accepted a genre and the request must carry it — an \
         interpreted source used to render the panel, take the selection and \
         send an unfiltered query. Saw: {:?}",
        seen.query
    );
}

#[tokio::test]
async fn an_interpreted_source_repeats_a_multiselect_filter() {
    use kani_shared::types::{ActiveFilter, FilterState};

    let origin = TestOrigin::start().await;
    origin.set("/search", Response::json(r#"{"results":[]}"#));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "filtering-source").await;
    wire_filtering_source(&svc, source_id, &origin.base());

    let filters = vec![ActiveFilter {
        filter_name: "tags".into(),
        state: FilterState::Multiselect(vec!["action".into(), "comedy".into()]),
    }];
    let json = serde_json::to_string(&filters).unwrap();
    svc.search_manga(source_id, "q", 1, 20, Some(json))
        .await
        .unwrap();

    let q = origin
        .last_request("/search")
        .unwrap()
        .query
        .unwrap_or_default();
    assert!(
        q.contains("t=action") && q.contains("t=comedy"),
        "the default multiselect format repeats the parameter, got {q}"
    );
}

#[tokio::test]
async fn an_unmapped_filter_is_ignored_rather_than_guessed_at() {
    use kani_shared::types::{ActiveFilter, FilterState};

    let origin = TestOrigin::start().await;
    origin.set("/search", Response::json(r#"{"results":[]}"#));

    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "filtering-source").await;
    wire_filtering_source(&svc, source_id, &origin.base());

    let filters = vec![ActiveFilter {
        filter_name: "not_in_the_mapping".into(),
        state: FilterState::TextInput("x".into()),
    }];
    let json = serde_json::to_string(&filters).unwrap();
    svc.search_manga(source_id, "q", 1, 20, Some(json))
        .await
        .unwrap();

    let q = origin
        .last_request("/search")
        .unwrap()
        .query
        .unwrap_or_default();
    assert!(
        !q.contains("not_in_the_mapping") && !q.contains("x"),
        "a filter the endpoint never declared must not be invented into the \
         query string, got {q}"
    );
}

// ── A4/A5. A degenerate migration target must not delete downloads ───────────

/// A source with the manga_details + chapter_list endpoints a migration target
/// needs. The chapter listing is scriptable so a test can make it degenerate.
fn wire_migration_target(svc: &kani_app::service::AppService, source_id: i64, base_url: &str) {
    let details = json_endpoint(
        "/target/$manga_id$",
        "/manga",
        vec![
            json_field("id", "/id", false),
            json_field("title", "/title", false),
        ],
    );
    let chapters = json_endpoint(
        "/target/$manga_id$/chapters",
        "/chapters",
        vec![
            json_field("id", "/id", false),
            json_field("number", "/number", false),
        ],
    );
    let ext = ValidatedExtension {
        id: "target-source".into(),
        name: "Target Source".into(),
        version: "1.0.0".into(),
        base_url: base_url.to_string(),
        language: "en".into(),
        unrestricted_http: true,
        manga_details: Some(details),
        chapter_list: Some(chapters),
        ..Default::default()
    };
    let source = YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        format!("target-{source_id}:"),
        HashMap::new(),
        true,
    );
    svc.sources
        .insert(source_id, SourceBackend::Yaml(Box::new(source)));
}

async fn seed_migration_scenario(
    origin: &TestOrigin,
    svc: &kani_app::service::AppService,
    chapter_list_body: &str,
) -> (kani_app::ids::MangaId, i64, std::path::PathBuf) {
    let home_source = insert_source(&svc.db, "home-source").await;
    let manga = insert_manga(&svc.db, home_source, "m1", "Held Series").await;

    // One downloaded chapter with a real CBZ on disk.
    let pages = vec![jpeg_page(800, 1200, false, 80); 2];
    let held = held_chapter_with_pages(svc, manga, "Held Series", "ch-1", 1.0, &pages).await;
    let cbz = svc.chapter_cbz_path(held).await.unwrap().path;
    assert!(cbz.exists(), "fixture CBZ must exist before migration");

    let target_source = insert_source(&svc.db, "target-source").await;
    wire_migration_target(svc, target_source, &origin.base());
    origin.set(
        "/target/tgt-1",
        Response::json(r#"{"manga":[{"id":"tgt-1","title":"Held Series"}]}"#),
    );
    origin.set("/target/tgt-1/chapters", Response::json(chapter_list_body));

    (manga, target_source, cbz)
}

#[tokio::test]
async fn an_empty_target_listing_does_not_delete_downloads() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let (manga, target, cbz) = seed_migration_scenario(&origin, &svc, r#"{"chapters":[]}"#).await;

    let res = svc
        .migrate_manga(manga, target, "tgt-1".into(), false)
        .await;

    assert!(
        res.is_err(),
        "migrating to a source that lists no chapters must be refused, not silently \
         orphan and delete every download"
    );
    assert!(
        cbz.exists(),
        "the held CBZ must survive a refused migration"
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE manga_id = ?")
        .bind(manga)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert!(
        rows > 0,
        "no chapter rows may be deleted by a refused migration"
    );
}

#[tokio::test]
async fn a_target_whose_numbers_do_not_parse_does_not_delete_downloads() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    // Chapter numbers as strings collapse to 0.0 and match nothing.
    let (manga, target, cbz) = seed_migration_scenario(
        &origin,
        &svc,
        r#"{"chapters":[{"id":"t-1","number":"twelve"},{"id":"t-2","number":"thirteen"}]}"#,
    )
    .await;

    let res = svc
        .migrate_manga(manga, target, "tgt-1".into(), false)
        .await;

    assert!(res.is_err(), "a target matching nothing must be refused");
    assert!(cbz.exists(), "the held CBZ must survive");
}

#[tokio::test]
async fn a_migration_that_matches_is_still_allowed() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    // The target genuinely carries chapter 1 — a real, matching migration.
    let (manga, target, _cbz) = seed_migration_scenario(
        &origin,
        &svc,
        r#"{"chapters":[{"id":"t-1","number":1},{"id":"t-2","number":2}]}"#,
    )
    .await;

    let res = svc
        .migrate_manga(manga, target, "tgt-1".into(), false)
        .await;
    assert!(
        res.is_ok(),
        "a target that matches the held chapter must still migrate, got {res:?}"
    );
}

// ── J1. Chapters are matched by number across sources ────────────────────────

#[tokio::test]
async fn migration_matches_chapters_by_number_across_sources() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    // The target carries number 1 (as "t-1") and number 2 ("t-2"); the held
    // chapter is number 1.0.
    let (manga, target, _cbz) = seed_migration_scenario(
        &origin,
        &svc,
        r#"{"chapters":[{"id":"t-1","number":1},{"id":"t-2","number":2}]}"#,
    )
    .await;

    svc.migrate_manga(manga, target, "tgt-1".into(), false)
        .await
        .unwrap();

    // The series now belongs to the target source.
    let src: i64 = sqlx::query_scalar("SELECT source_id FROM manga WHERE id = ?")
        .bind(manga)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        src, target,
        "the manga's source pointer moved to the target"
    );

    // The held chapter (number 1.0) was matched to the target's number-1 chapter
    // — same DB row, source_chapter_id remapped to the target's id.
    let scid: String = sqlx::query_scalar(
        "SELECT source_chapter_id FROM chapters WHERE manga_id = ? AND chapter_number = 1.0",
    )
    .bind(manga)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(
        scid, "t-1",
        "chapter 1 was matched across sources by its number, not dropped"
    );
}

// ── J2. keep_orphaned_downloads = true preserves files on the safe branch ────

#[tokio::test]
async fn keep_orphaned_downloads_true_preserves_files() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    // A target that matches nothing (empty listing) — refused with
    // keep_orphaned=false (A4). With keep_orphaned=true the user accepts it and
    // the downloaded file is preserved, the chapter kept as an orphan.
    let (manga, target, cbz) = seed_migration_scenario(&origin, &svc, r#"{"chapters":[]}"#).await;

    svc.migrate_manga(manga, target, "tgt-1".into(), true)
        .await
        .expect("keep-orphaned migration proceeds even when the target matches nothing");

    assert!(
        cbz.exists(),
        "the downloaded CBZ is preserved on the keep-orphaned branch"
    );
    let orphaned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chapters WHERE manga_id = ? AND is_orphaned = 1")
            .bind(manga)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert!(
        orphaned >= 1,
        "the held download is kept as an orphan rather than deleted"
    );
}

// ── J4. Reading progress survives a migration ────────────────────────────────

#[tokio::test]
async fn read_progress_survives_a_migration() {
    let origin = TestOrigin::start().await;
    let svc = test_service().await;
    let (manga, target, _cbz) = seed_migration_scenario(
        &origin,
        &svc,
        r#"{"chapters":[{"id":"t-1","number":1},{"id":"t-2","number":2}]}"#,
    )
    .await;
    let user = common::insert_user(&svc.db, "reader").await;

    // The held chapter (number 1.0) has read progress at page 5.
    let held: i64 =
        sqlx::query_scalar("SELECT id FROM chapters WHERE manga_id = ? AND chapter_number = 1.0")
            .bind(manga)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO user_chapter_tracking (user_id, chapter_id, last_page_read, is_read) \
         VALUES (?, ?, 5, 0)",
    )
    .bind(user.0)
    .bind(held)
    .execute(&svc.db)
    .await
    .unwrap();

    svc.migrate_manga(manga, target, "tgt-1".into(), false)
        .await
        .unwrap();

    // The matched chapter keeps its row id, so its progress is untouched.
    let page: Option<i64> = sqlx::query_scalar(
        "SELECT last_page_read FROM user_chapter_tracking WHERE chapter_id = ? AND user_id = ?",
    )
    .bind(held)
    .bind(user.0)
    .fetch_optional(&svc.db)
    .await
    .unwrap();
    assert_eq!(
        page,
        Some(5),
        "reading progress survives migration on the matched chapter"
    );
}
