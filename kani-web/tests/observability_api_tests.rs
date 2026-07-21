#![allow(clippy::unwrap_used)]
// Tests for the observability layer: request-trace IDs on every response.

mod common;
use common::{build_test_app, get_req, test_state};
use tower::ServiceExt;

#[tokio::test]
async fn every_response_carries_an_x_request_id_header() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/sources")).await.unwrap();

    let id = res
        .headers()
        .get("x-request-id")
        .expect("every response must carry x-request-id")
        .to_str()
        .unwrap();
    assert_eq!(id.len(), 36, "expected a hyphenated uuid v4, got {id}");
}

#[tokio::test]
async fn error_responses_also_carry_an_x_request_id_header() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/sources")).await.unwrap();

    assert!(
        res.status().is_client_error(),
        "unauthenticated request should be a 4xx, got {}",
        res.status()
    );
    assert!(
        res.headers().contains_key("x-request-id"),
        "error responses must still carry a trace id"
    );
}

#[tokio::test]
async fn inbound_x_request_id_is_echoed_back() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let supplied = "11111111-2222-3333-4444-555555555555";
    let req = axum::http::Request::builder()
        .uri("/rest/sources")
        .header("x-request-id", supplied)
        .body(axum::body::Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();

    assert_eq!(
        res.headers().get("x-request-id").unwrap().to_str().unwrap(),
        supplied,
        "a caller-supplied trace id must be propagated, not replaced"
    );
}

#[tokio::test]
async fn metrics_endpoint_renders_registered_kani_metrics_without_auth() {
    kani_web::metrics::describe();
    let app = kani_web::metrics::router();

    let res = app.oneshot(get_req("/metrics")).await.unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    for metric in [
        "kani_log_errors_total",
        "kani_sse_clients",
        "kani_jobs_running",
    ] {
        assert!(
            text.contains(metric),
            "{metric} should be pre-registered so it is scrapeable before first use"
        );
    }
    assert!(
        text.contains("# TYPE"),
        "expected prometheus exposition format"
    );
}

#[tokio::test]
async fn each_request_gets_a_distinct_generated_id() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let first = app.clone().oneshot(get_req("/rest/sources")).await.unwrap();
    let second = app.oneshot(get_req("/rest/sources")).await.unwrap();

    let a = first.headers().get("x-request-id").unwrap();
    let b = second.headers().get("x-request-id").unwrap();
    assert_ne!(a, b, "generated trace ids must be unique per request");
}
