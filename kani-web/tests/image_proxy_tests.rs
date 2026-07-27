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
        vec![Response::status(429), Response::image(vec![1, 2, 3, 4])],
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
        &[1, 2, 3, 4],
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
        Response::image(vec![9; 32]).delay(Duration::from_millis(200)),
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
