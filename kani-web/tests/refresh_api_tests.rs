#![allow(clippy::unwrap_used)]
// Tests for POST /manga/{id}/refresh with the new optional body.

mod common;
use axum::http::StatusCode;
use common::{authed_post, build_test_app, create_admin, login, test_state};
use tower::ServiceExt;

#[tokio::test]
async fn refresh_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/rest/manga/1/refresh")
        .body(axum::body::Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_returns_400_for_unknown_field_name() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Field validation happens before the service call, so the manga ID doesn't need to exist.
    let res = app
        .oneshot(authed_post(
            "/rest/manga/99999/refresh",
            &cookie,
            serde_json::json!({ "fields": ["bogus_field"] }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn refresh_with_partial_fields_reaches_service() {
    // Validation for known fields passes; service rejects because manga doesn't exist.
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/99999/refresh",
            &cookie,
            serde_json::json!({ "fields": ["description", "status"], "fetch_chapters": false }),
        ))
        .await
        .unwrap();

    // 404 (manga not found) confirms auth+field validation passed; the service rejected it.
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refresh_with_clear_overrides_reaches_service() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/99999/refresh",
            &cookie,
            serde_json::json!({ "clear_overrides": true }),
        ))
        .await
        .unwrap();

    // 404 confirms auth+validation passed; the service rejected it.
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn refresh_with_no_body_bypasses_validation_and_reaches_service() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Body-less POST (original behavior) — validation skipped, falls through to service.
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/rest/manga/99999/refresh")
        .header("Cookie", &cookie)
        .body(axum::body::Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();

    // 404 confirms auth passed and the body-less path works.
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
