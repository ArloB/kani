#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{authed_get, authed_post, body_json, build_test_app, create_admin, test_state};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn a_created_collection_is_listed_back() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let empty = app
        .clone()
        .oneshot(authed_get("/rest/collections", &cookie))
        .await
        .unwrap();
    assert_eq!(empty.status(), StatusCode::OK);
    let before = body_json(empty).await;
    let before = before.as_array().expect("collections must be an array");
    assert!(before.is_empty(), "a fresh instance has no collections");

    app.clone()
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

    let listed = app
        .oneshot(authed_get("/rest/collections", &cookie))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);

    let after = body_json(listed).await;
    let after = after.as_array().expect("collections must be an array");
    assert_eq!(after.len(), 1, "the created collection must be listed");
    assert_eq!(after[0]["name"], "Ongoing Manga");
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
