#![allow(clippy::unwrap_used)]
// Tests for the /rest/auth/* endpoints: login, logout, session round-trip,
// registration-enabled flag, and GET /auth/me auth enforcement.

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_json, build_test_app, create_admin, create_regular_user, get_req,
    login, post_json, test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn login_with_valid_credentials_returns_ok() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/auth/login",
            serde_json::json!({"username": username, "password": password}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["ok"], serde_json::json!(true));
}

#[tokio::test]
async fn login_with_invalid_password_returns_401() {
    let state = test_state().await;
    let (username, _) = create_admin(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/auth/login",
            serde_json::json!({"username": username, "password": "wrongpassword"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_with_unknown_user_returns_401() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/auth/login",
            serde_json::json!({"username": "nobody", "password": "anything"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_me_returns_user_for_authenticated_session() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_get("/rest/auth/me", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["username"], serde_json::json!(username));
}

#[tokio::test]
async fn auth_me_returns_401_without_session() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/auth/me")).await.unwrap();

    // auth_guard lets /rest/auth/* through; AuthGuard extractor returns 401.
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_invalidates_session() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/auth/logout",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(authed_get("/rest/auth/me", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn registration_enabled_returns_correct_flag() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/auth/registration-enabled"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // new_for_test sets registration_enabled = true
    assert_eq!(body["enabled"], serde_json::json!(true));
}

#[tokio::test]
async fn get_current_user_returns_full_user_for_authenticated_session() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "alice").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/auth/current_user", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["username"], serde_json::json!(username));
    assert!(
        body.get("password_hash").is_none(),
        "password hash must not be exposed"
    );
}
