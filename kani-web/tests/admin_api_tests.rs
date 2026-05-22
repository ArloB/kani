#![allow(clippy::unwrap_used)]
// Tests for /rest/admin/* endpoints: user CRUD, role grant/revoke, audit log.

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_array, body_json, build_test_app, create_admin,
    create_regular_user, get_req, login, post_json, test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn admin_list_users_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/users", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let users = body_array(res).await;
    assert!(
        !users.is_empty(),
        "at least the admin user should be listed"
    );
}

#[tokio::test]
async fn admin_list_users_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "alice").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/users", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("forbidden"));
}

#[tokio::test]
async fn admin_list_users_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/admin/users")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_create_user_returns_201_for_admin() {
    let state = test_state().await;
    let (admin_username, admin_password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, admin_username, admin_password).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/admin/users",
            &cookie,
            serde_json::json!({
                "username": "newuser",
                "email": "newuser@test.local",
                "password": "Password1234!",
                "roles": []
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["username"], serde_json::json!("newuser"));

    let list_res = app
        .oneshot(authed_get("/rest/admin/users", &cookie))
        .await
        .unwrap();
    let users = body_array(list_res).await;
    assert_eq!(users.len(), 2, "admin + newuser");
}

#[tokio::test]
async fn admin_create_user_returns_400_for_short_password() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/users",
            &cookie,
            serde_json::json!({
                "username": "short",
                "email": "short@test.local",
                "password": "abc",
                "roles": []
            }),
        ))
        .await
        .unwrap();

    // Password < 8 chars → 400 ValidationError.
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_audit_log_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/audit-log", &cookie))
        .await
        .unwrap();

    // Audit log always returns 200 (may be empty or contain login events).
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_create_user_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/admin/users",
            serde_json::json!({
                "username": "ghost",
                "email": "ghost@test.local",
                "password": "Password1234!",
                "roles": []
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_audit_log_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/admin/audit-log")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
