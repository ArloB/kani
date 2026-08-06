#![allow(clippy::unwrap_used)]

//! Group K — the image proxy's fetch path, driven end-to-end through a signed
//! token against a real upstream `TestOrigin`. Previously untestable because the
//! retry backoff, request timeout and size caps were hardcoded consts; they now
//! live in `AppState::proxy_config` (a `ProxyConfig`) which the test shortens.

mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{authed_get, build_test_app_with_proxy, create_admin, login, test_state};
use kani_shared_test::origin::{Body as OriginBody, Response, TestOrigin};
use kani_web::proxy::{ProxyConfig, make_proxy_url};
use kani_web::state::AppState;
use std::time::Duration;
use tower::ServiceExt;

/// Fast timings with room to override individual fields.
fn fast_proxy_config() -> ProxyConfig {
    ProxyConfig {
        base_delay: Duration::from_millis(1),
        retry_jitter: Duration::ZERO,
        min_host_interval: Duration::ZERO,
        ..ProxyConfig::default()
    }
}

fn signed_get(state: &AppState, upstream_url: &str, cookie: &str) -> Request<Body> {
    let signed = make_proxy_url(upstream_url, "http://ref.test/", &state.proxy_secret, None);
    authed_get(&signed, cookie)
}

// K1 — a retryable 429 is retried and the eventual 200 is served.
#[tokio::test]
async fn the_image_proxy_retries_a_429_then_succeeds() {
    let origin = TestOrigin::start().await;
    origin.script(
        "/img.jpg",
        vec![
            Response::status(429),
            Response::image(kani_shared_test::origin::jpeg_page(16, 16, false, 80)),
        ],
    );
    let mut state = test_state().await;
    state.proxy_config = fast_proxy_config();
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(signed_get(&state, &origin.url("/img.jpg"), &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        &body[..],
        &kani_shared_test::origin::jpeg_page(16, 16, false, 80)[..],
        "the retried request served the image"
    );
    assert_eq!(origin.hits("/img.jpg"), 2, "the 429 was retried once");
}

// K2 — a stalling upstream hits the request timeout and errors rather than hanging.
#[tokio::test]
async fn the_image_proxy_times_out_rather_than_hanging() {
    let origin = TestOrigin::start().await;
    origin.set("/img.jpg", Response::status(200).body(OriginBody::Stall));
    let mut state = test_state().await;
    state.proxy_config = ProxyConfig {
        request_timeout: Duration::from_millis(150),
        max_retries: 1,
        ..fast_proxy_config()
    };
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let res = tokio::time::timeout(
        Duration::from_secs(10),
        app.oneshot(signed_get(&state, &origin.url("/img.jpg"), &cookie)),
    )
    .await
    .expect("the proxy must return, not hang, on a stalling upstream")
    .unwrap();

    assert!(
        res.status().is_server_error(),
        "a timed-out upstream surfaces as an error, got {}",
        res.status()
    );
}

// K3 — concurrent requests for the same URL coalesce into a single upstream hit.
#[tokio::test]
async fn concurrent_proxy_requests_for_one_url_coalesce() {
    let origin = TestOrigin::start().await;
    // Slow enough that all five requests are in-flight before the first resolves.
    origin.set(
        "/img.jpg",
        Response::image(kani_shared_test::origin::jpeg_page(16, 16, false, 80))
            .delay(Duration::from_millis(200)),
    );
    let mut state = test_state().await;
    state.proxy_config = fast_proxy_config();
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let mut handles = Vec::new();
    for _ in 0..5 {
        let app = app.clone();
        let req = signed_get(&state, &origin.url("/img.jpg"), &cookie);
        handles.push(tokio::spawn(async move { app.oneshot(req).await }));
    }
    for h in handles {
        let res = h.await.unwrap().unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    assert_eq!(
        origin.hits("/img.jpg"),
        1,
        "five concurrent requests hit the upstream once"
    );
}

// K4 — an oversized body is refused at the configured cap.
#[tokio::test]
async fn an_oversized_image_is_capped() {
    let origin = TestOrigin::start().await;
    origin.set("/img.jpg", Response::image(vec![7; 4096]));
    let mut state = test_state().await;
    state.proxy_config = ProxyConfig {
        max_image_bytes: 64,
        ..fast_proxy_config()
    };
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(signed_get(&state, &origin.url("/img.jpg"), &cookie))
        .await
        .unwrap();

    assert!(
        res.status().is_server_error(),
        "a body past the cap is refused, got {}",
        res.status()
    );
}

// A CDN serving a real image as `application/octet-stream` used to be refused by
// the declared-type gate — the proxy answered 500 and the page showed a broken
// image. Observed against a real source during a browser sweep.
#[tokio::test]
async fn an_image_served_as_octet_stream_is_proxied() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/img",
        Response::ok(kani_shared_test::origin::jpeg_page(32, 48, false, 80))
            .header("Content-Type", "application/octet-stream"),
    );
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(signed_get(&state, &origin.url("/img"), &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK, "a real JPEG must be served");
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/jpeg"),
        "the served type comes from the bytes, not from the upstream's label"
    );
}

// The security half: a label must not buy passage. This is stronger than the
// old gate, which trusted `Content-Type: image/png` outright.
#[tokio::test]
async fn a_page_labelled_as_an_image_is_not_proxied() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/not-img",
        Response::ok(b"<html><body>Just a moment...</body></html>".to_vec())
            .header("Content-Type", "image/png"),
    );
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(signed_get(&state, &origin.url("/not-img"), &cookie))
        .await
        .unwrap();

    assert!(
        !res.status().is_success(),
        "an HTML page claiming to be a PNG must not be served as an image, got {}",
        res.status()
    );
}

// K5 — the reader asks for a byte range; the upstream ignores it and answers
// `200` with the whole body. The proxy must pass that through as a complete
// `200` rather than mislabelling it `206 Partial Content` (which would tell the
// reader it received only a slice) or failing the request outright.
#[tokio::test]
async fn an_upstream_that_ignores_range_still_serves_the_reader() {
    let origin = TestOrigin::start().await;
    let image = vec![9u8; 2048];
    origin.set("/img.jpg", Response::image(image.clone()));
    origin.ignore_range(true);

    let mut state = test_state().await;
    state.proxy_config = fast_proxy_config();
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_proxy(state.clone()).await;
    let cookie = login(&app, u, p).await;

    let signed = make_proxy_url(
        &origin.url("/img.jpg"),
        "http://ref.test/",
        &state.proxy_secret,
        None,
    );
    let mut req = authed_get(&signed, &cookie);
    req.headers_mut().insert(
        axum::http::header::RANGE,
        axum::http::HeaderValue::from_static("bytes=0-511"),
    );

    let res = app.oneshot(req).await.unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "an ignored range is a complete response, not partial content"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        body.len(),
        image.len(),
        "the reader still receives the whole usable image"
    );
}
