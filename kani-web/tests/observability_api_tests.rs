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
async fn metrics_endpoint_is_denied_when_no_token_is_configured() {
    kani_web::metrics::describe();
    let app = kani_web::metrics::router();

    let res = app.oneshot(get_req("/metrics")).await.unwrap();

    assert_eq!(
        res.status(),
        axum::http::StatusCode::UNAUTHORIZED,
        "metrics disclose extension and upstream host names; they must not be \
         readable until an operator configures a token"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("KANI_METRICS_TOKEN"),
        "the refusal should say how to enable scraping, got: {text}"
    );
}

#[tokio::test]
async fn registered_kani_metrics_are_present_in_the_exposition() {
    kani_web::metrics::describe();
    let handle = &kani_web::metrics::prometheus().1;
    let text = handle.render();
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

#[tokio::test]
async fn diagnostics_returns_payload_for_admin() {
    let state = test_state().await;
    let (username, password) = common::create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(common::authed_get("/rest/admin/diagnostics", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = common::body_json(res).await;
    assert!(body["version"].is_string(), "version missing: {body}");
    assert!(body["uptime_secs"].is_number());
    assert!(body["extensions"].is_array());
    assert!(
        body["browser"]["calls_total"].is_number(),
        "browser section (plan 02 stats) missing: {body}"
    );
}

#[tokio::test]
async fn diagnostics_requires_authentication() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/admin/diagnostics"))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn diagnostics_is_forbidden_for_regular_users() {
    let state = test_state().await;
    let (username, password) = common::create_regular_user(&state, "plain").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(common::authed_get("/rest/admin/diagnostics", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn support_bundle_returns_a_zip_with_expected_entries() {
    let state = test_state().await;
    let (username, password) = common::create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(common::authed_get("/rest/admin/support-bundle", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let disposition = res
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        disposition.contains("kani-support-") && disposition.contains(".zip"),
        "unexpected content-disposition: {disposition}"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_string())
        .collect();

    for expected in [
        "kani_info.json",
        "config.json",
        "db_schema.sql",
        "extensions.json",
        "diagnostics.json",
        "logs.jsonl",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "missing {expected} in {names:?}"
        );
    }
}

#[tokio::test]
async fn support_bundle_requires_authentication() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/admin/support-bundle"))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn system_update_reports_current_version_for_authed_user() {
    let state = test_state().await;
    let (username, password) = common::create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(common::authed_get("/rest/system/update", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::OK);
    let body = common::body_json(res).await;
    assert_eq!(
        body["current"].as_str().unwrap(),
        env!("CARGO_PKG_VERSION"),
        "should report the running version"
    );
    assert_eq!(
        body["update_available"], false,
        "no check has run, so no update should be claimed"
    );
}

#[tokio::test]
async fn system_update_requires_authentication() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/system/update")).await.unwrap();

    assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
}
