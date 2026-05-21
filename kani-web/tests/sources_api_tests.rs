#![allow(clippy::unwrap_used)]
// Tests for /rest/sources endpoints: list, get, add (admin-only), delete.
// WASM upload is tested separately (it requires multipart + binary fixture).

mod common;
use axum::http::StatusCode;
use common::{authed_get, authed_post, body_array, body_json, build_test_app, create_admin, create_regular_user, get_req, login, post_json, test_state};
use tower::ServiceExt;

#[tokio::test]
async fn list_sources_returns_empty_list_on_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/sources", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let sources = body_array(res).await;
    assert!(sources.is_empty(), "fresh DB should have no sources");
}

#[tokio::test]
async fn list_sources_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/sources"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_source_returns_404_for_missing_id() {
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

#[tokio::test]
async fn add_source_requires_source_install_permission() {
    let state = test_state().await;
    // Regular user role does not have source:install permission.
    let (username, password) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "my-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("forbidden"));
}

#[tokio::test]
async fn add_source_returns_201_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "test-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(body["id"].is_number(), "response must contain numeric id, got: {body}");

    // Verify it appears in the list.
    let list_res = app
        .oneshot(authed_get("/rest/sources", &cookie))
        .await
        .unwrap();
    let sources = body_array(list_res).await;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["name"], serde_json::json!("test-source"));
}

#[tokio::test]
async fn add_source_returns_400_for_empty_name() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": ""}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_source_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/sources/1"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn add_source_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/sources",
            serde_json::json!({"name": "test-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_source_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Create a source first so there is something to fetch.
    let create_res = app
        .clone()
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "fetch-me"}),
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let created = body_json(create_res).await;
    let id = created["id"].as_i64().expect("id must be numeric");

    let res = app
        .oneshot(authed_get(&format!("/rest/sources/{id}"), &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], serde_json::json!("fetch-me"));
}
