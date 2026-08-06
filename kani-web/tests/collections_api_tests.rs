#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_json, build_test_app, create_admin, get_req, post_json,
    test_state,
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn list_collections_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/collections", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_collections_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/collections")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_collection_returns_201_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/collections",
            &cookie,
            json!({
                "name": "Ongoing Manga",
                "rule": { "op": "status", "value": 1 },
                "sort_order": 0
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["name"], "Ongoing Manga");
}

#[tokio::test]
async fn create_collection_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/collections",
            json!({ "name": "Test", "rule": { "op": "status", "value": 1 }, "sort_order": 0 }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_collection_manga_returns_404_for_missing_collection() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/collections/999999/manga", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_collection_manga_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/collections/1/manga"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
