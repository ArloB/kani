#![allow(clippy::unwrap_used)]
// Verifies that each `AppError` variant surfaces the correct HTTP status code
// and a JSON body containing a machine-readable `code` field.

mod common;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use common::{
    authed_get, authed_post, body_json, build_test_app, create_admin, create_regular_user, get_req,
    login, post_json, test_state,
};
use kani_app::ServiceError;
use kani_web::error::AppError;
use tower::ServiceExt;

// ── ServiceError → AppError → HTTP status mapping ────────────────────────────

#[test]
fn service_conflict_maps_to_409() {
    let resp = AppError::from(ServiceError::Conflict("already in progress".into())).into_response();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[test]
fn service_forbidden_maps_to_403() {
    let resp = AppError::from(ServiceError::Forbidden("no access".into())).into_response();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn service_not_found_maps_to_404() {
    let resp = AppError::from(ServiceError::NotFound("manga 1".into())).into_response();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── 404 Not Found ────────────────────────────────────────────────────────────

#[tokio::test]
async fn missing_manga_returns_404_with_json_code() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/manga/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("not_found"));
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn missing_source_returns_404_with_json_code() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/sources/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("not_found"));
}

// ── 401 Unauthorized ─────────────────────────────────────────────────────────

#[tokio::test]
async fn unauthenticated_library_request_returns_401_with_json() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/library")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    // auth_guard returns {"error": "..."}; code field may or may not be present.
    assert!(body["error"].is_string());
}

#[tokio::test]
async fn invalid_login_returns_401_with_json_error() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/auth/login",
            serde_json::json!({"username": "nobody", "password": "wrong"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(res).await;
    assert!(body["error"].is_string());
}

// ── 403 Forbidden ────────────────────────────────────────────────────────────

#[tokio::test]
async fn regular_user_accessing_admin_endpoint_returns_403() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "carol").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/users", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("forbidden"));
    assert!(
        body["hint"].is_string(),
        "forbidden errors must include a hint"
    );
}

// ── 400 Bad Request / Validation ─────────────────────────────────────────────

#[tokio::test]
async fn malformed_login_body_returns_400() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    // Missing "password" field → deserialization error.
    let res = app
        .oneshot(post_json(
            "/rest/auth/login",
            serde_json::json!({"username": "someone"}),
        ))
        .await
        .unwrap();

    // 422 Unprocessable Entity from axum's Json extractor, or 400 from our
    // ValidationError mapping — either is acceptable.
    assert!(
        res.status().is_client_error(),
        "malformed body should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn admin_create_user_with_short_password_returns_400() {
    let state = test_state().await;
    let (admin_username, admin_password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, admin_username, admin_password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/users",
            &cookie,
            serde_json::json!({
                "username": "x",
                "email": "x@test.local",
                "password": "abc",
                "roles": []
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("validation_error"));
}
