#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{authed_get, body_json, build_test_app, create_admin, login, put_json, test_state};
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

    assert!(
        !res.status().is_success(),
        "expected error for non-existent chapter, got {}",
        res.status()
    );
}

#[tokio::test]
async fn set_read_status_returns_204_for_empty_chapter_list() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

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
async fn get_manga_chapters_returns_empty_for_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/manga/999999/chapters?page=1", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(
        body["chapters"]
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false),
        "expected empty chapters array, got: {body}"
    );
}

#[tokio::test]
async fn get_manga_chapters_invalid_page_returns_400() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/manga/1/chapters?page=0", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
