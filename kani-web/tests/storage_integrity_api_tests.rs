#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_array, body_json, build_test_app, create_admin,
    create_regular_user, get_req, post_json, test_state,
};
use serde_json::json;
use tower::ServiceExt;

// ── GET /rest/admin/storage/stats ─────────────────────────────────────────────

#[tokio::test]
async fn admin_storage_stats_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/storage/stats", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("library_used_bytes").is_some());
    assert!(body.get("total_manga").is_some());
    assert!(body.get("total_downloads").is_some());
}

#[tokio::test]
async fn admin_storage_stats_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/admin/storage/stats"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_storage_stats_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "alice").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/storage/stats", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ── GET /rest/admin/storage/stats/history ────────────────────────────────────

#[tokio::test]
async fn admin_storage_stats_history_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/storage/stats/history", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let _body = body_array(res).await;
}

#[tokio::test]
async fn admin_storage_stats_history_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/admin/storage/stats/history"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_storage_stats_history_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/storage/stats/history", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ── POST /rest/admin/library/scrub ───────────────────────────────────────────
// Supersedes /admin/library/integrity-check, which was removed with
// check_library/cleanup_orphans. The scrub is a job, so it returns 202 + job_id
// rather than an inline report.

#[tokio::test]
async fn admin_scrub_returns_202_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/library/scrub",
            &cookie,
            json!({ "depth": "quick", "fix": false }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::ACCEPTED);
    let body = body_json(res).await;
    assert!(
        body.get("job_id").is_some(),
        "the caller needs the id to follow progress over SSE"
    );
}

#[tokio::test]
async fn admin_scrub_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json("/rest/admin/library/scrub", json!({})))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_scrub_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "carol").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post("/rest/admin/library/scrub", &cookie, json!({})))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_scrub_rejects_an_unknown_depth() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/library/scrub",
            &cookie,
            json!({ "depth": "thorough" }),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "an unrecognised depth must not silently become 'quick'"
    );
}

#[tokio::test]
async fn the_removed_integrity_check_endpoint_is_gone() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/library/integrity-check",
            &cookie,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "the old endpoint deleted orphans as a side effect of ?fix=true; it must \
         not linger once deletion needs an explicit path list"
    );
}
