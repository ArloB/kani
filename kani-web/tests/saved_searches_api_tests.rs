#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_delete, authed_get, authed_post, body_json, build_test_app, create_admin, get_req,
    post_json, test_state,
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn list_saved_searches_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/saved-searches", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn list_saved_searches_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/saved-searches")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_saved_search_returns_201_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/saved-searches",
            &cookie,
            json!({
                "name": "Ongoing",
                "query_json": "{\"status_filter\":1}"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["name"], "Ongoing");
}

#[tokio::test]
async fn create_saved_search_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/saved-searches",
            json!({ "name": "Test", "query_json": "{}" }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_saved_search_returns_404_for_missing_id() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_delete("/rest/saved-searches/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_saved_search_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(common::delete_req("/rest/saved-searches/1"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
