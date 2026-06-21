#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_delete, authed_get, authed_post, body_json, build_test_app, create_admin,
    create_regular_user, get_req, login, post_json, test_state,
};
use tower::ServiceExt;

use ed25519_dalek::SigningKey;
use kani_app::source::signing::pubkey_b64;

fn gen_key() -> SigningKey {
    let bytes: [u8; 32] = rand::random();
    SigningKey::from_bytes(&bytes)
}

fn pk_b64(key: &SigningKey) -> String {
    pubkey_b64(key)
}

/// Insert a repo row directly — bypasses the HTTP fetch in `add_repo` and
/// the HTTPS-only URL validator. Used to seed test state for GET/DELETE/etc.
async fn seed_repo(state: &kani_web::state::AppState, url: &str, name: &str, pk: &str) -> i64 {
    let index_json = serde_json::json!({"name": name, "maintainer_key": pk, "extensions": []});
    let index_str = serde_json::to_string(&index_json).unwrap();
    sqlx::query_scalar!(
        "INSERT INTO repo_trust (url, name, maintainer_key, index_cache) VALUES (?, ?, ?, ?) RETURNING id",
        url, name, pk, index_str
    )
    .fetch_one(&state.db)
    .await
    .unwrap()
}

// ── /sources/repos — auth triplets ───────────────────────────────────────────

#[tokio::test]
async fn list_repos_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/rest/sources/repos")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_repos_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "bob_repos").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_get("/rest/sources/repos", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_repos_returns_empty_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_get("/rest/sources/repos", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.as_array().map(|a| a.is_empty()).unwrap_or(false));
}

#[tokio::test]
async fn add_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(post_json(
            "/rest/sources/repos",
            serde_json::json!({"url": "https://example.com"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn add_repo_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "charlie_repos").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_post(
            "/rest/sources/repos",
            &cookie,
            serde_json::json!({"url": "https://example.com"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn get_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/rest/sources/repos/1")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_repo_returns_404_for_missing_id() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_get("/rest/sources/repos/99999", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/rest/sources/repos/1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(post_json(
            "/rest/sources/repos/1/refresh",
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_repo_extensions_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(get_req("/rest/sources/repos/1/extensions"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── /admin/sources/blocked-repos — auth triplets ─────────────────────────────

#[tokio::test]
async fn list_blocked_repos_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(get_req("/rest/admin/sources/blocked-repos"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_blocked_repos_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "dave_repos").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_get("/rest/admin/sources/blocked-repos", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_blocked_repos_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_get("/rest/admin/sources/blocked-repos", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn block_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(post_json(
            "/rest/admin/sources/blocked-repos",
            serde_json::json!({"url": "https://bad.example.com", "reason": "test"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn block_repo_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "eve_repos").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_post(
            "/rest/admin/sources/blocked-repos",
            &cookie,
            serde_json::json!({"url": "https://bad.example.com", "reason": "test"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ── /sources/install — auth triplets ─────────────────────────────────────────

#[tokio::test]
async fn install_from_repo_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(post_json(
            "/rest/sources/install",
            serde_json::json!({"repo_id": 1, "extension_id": "x"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn install_from_repo_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "frank_repos").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;
    let res = app
        .oneshot(authed_post(
            "/rest/sources/install",
            &cookie,
            serde_json::json!({"repo_id": 1, "extension_id": "x"}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ── Blocked repo at REST layer ────────────────────────────────────────────────

#[tokio::test]
async fn add_blocked_repo_returns_403() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state.clone()).await;
    let cookie = login(&app, username, password).await;

    // block_repo bypasses URL validation — just needs the URL to be in DB.
    state
        .service
        .block_repo("https://evil-repo.example.com", "blocked in test", None)
        .await
        .unwrap();

    let res = app
        .oneshot(authed_post(
            "/rest/sources/repos",
            &cookie,
            serde_json::json!({"url": "https://evil-repo.example.com"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ── Add-repo URL validation ───────────────────────────────────────────────────

#[tokio::test]
async fn add_repo_with_non_https_url_returns_400() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/sources/repos",
            &cookie,
            serde_json::json!({"url": "http://insecure.example.com"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ── GET/DELETE/REFRESH with seeded repo ──────────────────────────────────────

#[tokio::test]
async fn get_repo_returns_200_for_seeded_repo() {
    let state = test_state().await;
    let key = gen_key();
    let repo_id = seed_repo(&state, "https://example-repo.com", "My Repo", &pk_b64(&key)).await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/sources/repos/{repo_id}"),
            &cookie,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], "My Repo");
    assert_eq!(body["url"], "https://example-repo.com");
}

#[tokio::test]
async fn delete_repo_returns_no_content() {
    let state = test_state().await;
    let key = gen_key();
    let repo_id = seed_repo(
        &state,
        "https://to-delete.example.com",
        "Repo",
        &pk_b64(&key),
    )
    .await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_delete(
            &format!("/rest/sources/repos/{repo_id}"),
            &cookie,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn list_repo_extensions_returns_empty_for_seeded_repo() {
    let state = test_state().await;
    let key = gen_key();
    let repo_id = seed_repo(
        &state,
        "https://ext-list.example.com",
        "Ext Repo",
        &pk_b64(&key),
    )
    .await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/sources/repos/{repo_id}/extensions"),
            &cookie,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.as_array().map(|a| a.is_empty()).unwrap_or(false));
}

// ── admin block/unblock CRUD ─────────────────────────────────────────────────

#[tokio::test]
async fn admin_block_and_delete_blocked_repo() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let block_res = app
        .clone()
        .oneshot(authed_post(
            "/rest/admin/sources/blocked-repos",
            &cookie,
            serde_json::json!({
                "url": "https://block-me.example.com",
                "reason": "integration test"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(block_res.status(), StatusCode::NO_CONTENT);

    let list_res = app
        .clone()
        .oneshot(authed_get("/rest/admin/sources/blocked-repos", &cookie))
        .await
        .unwrap();
    let list = body_json(list_res).await;
    let entries = list.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    let id = entries[0]["id"].as_i64().unwrap();

    let del_res = app
        .oneshot(authed_delete(
            &format!("/rest/admin/sources/blocked-repos/{id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(del_res.status(), StatusCode::NO_CONTENT);
}
