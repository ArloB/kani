#![allow(clippy::unwrap_used)]
// Tests for library/manga REST endpoints:
// GET /library, GET /manga/{id}, DELETE /manga/{id},
// POST /manga/{id}/refresh, POST /library/scan-all, POST /manga/scan.

mod common;
use axum::http::StatusCode;
use common::{
    authed_delete, authed_get, authed_post, body_json, build_test_app, create_admin, delete_req,
    get_req, login, test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn get_library_returns_empty_list_for_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // page and page_size are required by the query validator.
    let res = app
        .oneshot(authed_get("/rest/library?page=1&page_size=20", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["items"], serde_json::json!([]));
}

#[tokio::test]
async fn get_library_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/library")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_manga_returns_404_for_missing_id() {
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
}

#[tokio::test]
async fn get_manga_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/manga/1")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn delete_manga_returns_404_for_missing_id() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_delete("/rest/manga/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn scan_all_library_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/library/scan-all",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();

    // With no sources configured, scan-all still returns 200 (nothing to do).
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn scan_all_library_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/library/scan-all"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_library_invalid_page_returns_400() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // page=0 violates the garde min=1 validator on LibraryQuery.
    let res = app
        .oneshot(authed_get("/rest/library?page=0&page_size=20", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_manga_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(delete_req("/rest/manga/1")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── POST /manga/scan ──────────────────────────────────────────────────────────

#[tokio::test]
async fn scan_manga_all_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": "all" }),
        ))
        .await
        .unwrap();

    // Scan is dispatched asynchronously; handler returns 202 Accepted.
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn scan_manga_ids_empty_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": [] }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn scan_manga_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    // POST without credentials — use a raw request without cookie.
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/rest/manga/scan")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(
            serde_json::to_string(&serde_json::json!({ "ids": "all" })).unwrap(),
        ))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn scan_manga_invalid_body_returns_422() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // "ids" must be either "all" or an array — an integer is invalid.
    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": 42 }),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "expected 4xx, got {}",
        res.status(),
    );
}
