#![allow(clippy::unwrap_used)]
// Tests for chapter-level REST endpoints:
// PUT /chapter/{id}/progress, PUT /chapters/read_status, GET /manga/{id}/chapters.
// WASM-driven download (POST /chapter/{id}/download) requires a real source with
// WASM loaded and is out of host-side REST test scope.

mod common;
use axum::http::StatusCode;
use common::{authed_get, body_json, build_test_app, create_admin, get_req, login, put_json, test_state};
use tower::ServiceExt;

#[tokio::test]
async fn set_chapter_progress_returns_error_for_missing_chapter() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            "/rest/chapter/999999/progress",
            &cookie,
            serde_json::json!({"page": 5}),
        ))
        .await
        .unwrap();

    // FK constraint violation on non-existent chapter_id → error response.
    assert!(
        !res.status().is_success(),
        "expected error for non-existent chapter, got {}",
        res.status()
    );
}

#[tokio::test]
async fn set_chapter_progress_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/chapter/1/progress"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_read_status_returns_204_for_empty_chapter_list() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Marking an empty chapter list as read is a valid no-op.
    let res = app
        .oneshot(put_json(
            "/rest/chapters/read_status",
            &cookie,
            serde_json::json!({"chapter_ids": [], "is_read": true}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn set_read_status_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/chapters/read_status"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_manga_chapters_returns_empty_for_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // page=1 is required by the query validator.
    let res = app
        .oneshot(authed_get("/rest/manga/999999/chapters?page=1", &cookie))
        .await
        .unwrap();

    // Non-existent manga → empty chapter list (the DB returns no rows).
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(
        body["chapters"].as_array().map(|a| a.is_empty()).unwrap_or(false),
        "expected empty chapters array, got: {body}"
    );
}

#[tokio::test]
async fn get_manga_chapters_invalid_page_returns_400() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // page=0 violates the garde min=1 validator.
    let res = app
        .oneshot(authed_get("/rest/manga/1/chapters?page=0", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_manga_chapters_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/manga/1/chapters"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
