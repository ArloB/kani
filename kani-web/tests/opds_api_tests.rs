#![allow(clippy::unwrap_used)]
// Tests for OPDS catalog endpoints mounted at /opds.
// OPDS uses per-handler auth (session cookie OR HTTP Basic auth).
// Endpoints: GET /opds, /opds/catalogue, /opds/manga/{id},
//            /opds/search, /opds/opensearch.

mod common;
use axum::http::StatusCode;
use axum::{body::Body, http::Request};
use common::{
    basic_auth, build_test_app_with_opds, create_admin, insert_manga, insert_source, login,
    test_state,
};
use http_body_util::BodyExt as _;
use tower::ServiceExt;

// ── Helper builders ────────────────────────────────────────────────────────────

fn opds_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn opds_authed_req(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn opds_basic_auth_req(uri: &str, username: &str, password: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", basic_auth(username, password))
        .body(Body::empty())
        .unwrap()
}

// ── GET /opds (root feed) ─────────────────────────────────────────────────────

#[tokio::test]
async fn opds_root_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app_with_opds(state).await;

    let res = app.oneshot(opds_req("/opds")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // OPDS 401 must advertise Basic auth realm
    let www_auth = res
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        www_auth.contains("Basic"),
        "Expected WWW-Authenticate: Basic, got: {www_auth}"
    );
}

#[tokio::test]
async fn opds_root_returns_200_with_session_auth() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("atom+xml"),
        "Expected Atom XML content-type, got: {content_type}"
    );
}

#[tokio::test]
async fn opds_root_returns_200_with_basic_auth() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;

    let res = app
        .oneshot(opds_basic_auth_req("/opds", username, password))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn opds_root_returns_401_with_wrong_basic_auth() {
    let state = test_state().await;
    let (_username, _password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;

    let res = app
        .oneshot(opds_basic_auth_req("/opds", "admin", "wrongpassword"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── GET /opds/catalogue ───────────────────────────────────────────────────────

#[tokio::test]
async fn opds_catalogue_returns_200_for_empty_library() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds/catalogue", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn opds_catalogue_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app_with_opds(state).await;

    let res = app.oneshot(opds_req("/opds/catalogue")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn opds_catalogue_includes_seeded_manga() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let src = insert_source(&state.db, "src").await;
    insert_manga(&state.db, src, "m1", "Dragon Ball").await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds/catalogue", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    // The Atom feed XML must mention the manga title
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let xml = std::str::from_utf8(&bytes).unwrap();
    assert!(
        xml.contains("Dragon Ball"),
        "Feed should contain seeded manga title"
    );
}

// ── GET /opds/manga/{id} ──────────────────────────────────────────────────────

#[tokio::test]
async fn opds_manga_returns_404_for_missing_id() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds/manga/999999", &cookie))
        .await
        .unwrap();

    // Service returns NotFound which maps to 404
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn opds_manga_returns_200_for_existing_manga() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let src = insert_source(&state.db, "src").await;
    let manga_id = insert_manga(&state.db, src, "m1", "Naruto").await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req(&format!("/opds/manga/{manga_id}"), &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn opds_manga_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app_with_opds(state).await;

    let res = app.oneshot(opds_req("/opds/manga/1")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── GET /opds/search ──────────────────────────────────────────────────────────

#[tokio::test]
async fn opds_search_returns_200_with_auth() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds/search?q=dragon+ball", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn opds_search_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app_with_opds(state).await;

    let res = app.oneshot(opds_req("/opds/search?q=test")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── GET /opds/opensearch ──────────────────────────────────────────────────────

#[tokio::test]
async fn opds_opensearch_returns_200_with_auth() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds/opensearch", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("xml"),
        "Expected XML content-type for OpenSearch, got: {content_type}"
    );
}

#[tokio::test]
async fn opds_opensearch_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app_with_opds(state).await;

    let res = app.oneshot(opds_req("/opds/opensearch")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── Feed shape assertions ─────────────────────────────────────────────────────

#[tokio::test]
async fn opds_root_feed_contains_catalogue_link() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app_with_opds(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(opds_authed_req("/opds", &cookie))
        .await
        .unwrap();

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let xml = std::str::from_utf8(&bytes).unwrap();
    assert!(
        xml.contains("/opds/catalogue"),
        "Root feed should link to catalogue"
    );
    assert!(
        xml.contains("urn:kani:root"),
        "Root feed should have expected id"
    );
}
