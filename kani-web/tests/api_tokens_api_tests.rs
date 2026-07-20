#![allow(clippy::unwrap_used)]
// Tests for the self-service API-token CRUD at /rest/me/api-tokens.

mod common;
use axum::http::StatusCode;
use axum::{body::Body, http::Request};
use common::{
    authed_delete, authed_get, authed_post, body_json, build_test_app_with_opds, create_admin,
    create_regular_user, login, test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn list_requires_auth() {
    let state = test_state().await;
    create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/rest/me/api-tokens")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_list_revoke_roundtrip() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, u, p).await;

    // Create.
    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "reader" }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = body_json(res).await;
    let raw = created["raw_token"].as_str().unwrap().to_owned();
    let id = created["id"].as_str().unwrap().to_owned();
    assert!(raw.starts_with("kani_"));

    // List shows it, without any raw token or hash.
    let res = app
        .clone()
        .oneshot(authed_get("/rest/me/api-tokens", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list = body_json(res).await;
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "reader");
    assert!(arr[0].get("raw_token").is_none());
    assert!(arr[0].get("token_hash").is_none());

    // Revoke.
    let res = app
        .clone()
        .oneshot(authed_delete(&format!("/rest/me/api-tokens/{id}"), &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // List is now empty.
    let res = app
        .clone()
        .oneshot(authed_get("/rest/me/api-tokens", &cookie))
        .await
        .unwrap();
    let list = body_json(res).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn create_rejects_empty_name() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "   " }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn revoked_token_no_longer_authenticates_on_opds() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "reader" }),
        ))
        .await
        .unwrap();
    let created = body_json(res).await;
    let raw = created["raw_token"].as_str().unwrap().to_owned();
    let id = created["id"].as_str().unwrap().to_owned();

    // Token works against OPDS before revoke.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/opds")
                .header("Authorization", format!("Bearer {raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Revoke, then it is rejected.
    app.clone()
        .oneshot(authed_delete(&format!("/rest/me/api-tokens/{id}"), &cookie))
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/opds")
                .header("Authorization", format!("Bearer {raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_are_isolated_from_each_others_tokens() {
    let state = test_state().await;
    let (au, ap) = create_admin(&state).await;
    let (bu, bp) = create_regular_user(&state, "bob").await;
    let app = build_test_app_with_opds(state).await;

    let admin_cookie = login(&app, au, ap).await;
    let bob_cookie = login(&app, bu, bp).await;

    // Admin creates a token.
    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/me/api-tokens",
            &admin_cookie,
            serde_json::json!({ "name": "admin-token" }),
        ))
        .await
        .unwrap();
    let id = body_json(res).await["id"].as_str().unwrap().to_owned();

    // Bob cannot see it.
    let res = app
        .clone()
        .oneshot(authed_get("/rest/me/api-tokens", &bob_cookie))
        .await
        .unwrap();
    assert!(body_json(res).await.as_array().unwrap().is_empty());

    // Bob cannot revoke it → 404.
    let res = app
        .clone()
        .oneshot(authed_delete(
            &format!("/rest/me/api-tokens/{id}"),
            &bob_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
