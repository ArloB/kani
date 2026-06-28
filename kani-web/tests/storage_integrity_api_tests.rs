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

// ── POST /rest/admin/library/integrity-check ──────────────────────────────────

#[tokio::test]
async fn admin_integrity_check_returns_200_for_admin() {
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

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("orphaned_files").is_some());
    assert!(body.get("missing_files").is_some());
    assert!(body.get("cover_mismatches").is_some());
    assert!(body.get("db_chapter_count").is_some());
    assert!(body.get("disk_file_count").is_some());
}

#[tokio::test]
async fn admin_integrity_check_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json("/rest/admin/library/integrity-check", json!({})))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_integrity_check_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "carol").await;
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

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_integrity_check_fix_mode_returns_cleanup_result() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/library/integrity-check?fix=true",
            &cookie,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("removed_count").is_some());
    assert!(body.get("failed_count").is_some());
    assert!(body.get("dry_run").is_some());
    assert_eq!(body["dry_run"], false);
}
