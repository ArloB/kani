#![allow(clippy::unwrap_used)]

//! Group C — repo index refresh under a conditional-GET 304. `refresh_repo` uses
//! `safe_get_conditional`; a 304 must reuse the cached index (C7) and, with no
//! cache to fall back on, must error rather than treat the repo as empty (C8).

mod common;
use common::test_service;
use kani_app::service::AppService;
use kani_shared_test::origin::{Response, TestOrigin};

const INDEX_JSON: &str = r#"{"name":"Test Repo","maintainer_key":"KEY","extensions":[]}"#;

async fn seed_repo(svc: &AppService, url: &str, index_cache: Option<&str>) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO repo_trust (url, name, maintainer_key, index_cache) \
         VALUES (?, 'Test Repo', 'KEY', ?) RETURNING id",
    )
    .bind(url)
    .bind(index_cache)
    .fetch_one(&svc.db)
    .await
    .unwrap()
}

// C7 — a 304 reuses the cached index unchanged and bumps the refresh timestamp.
#[tokio::test]
async fn a_304_yields_the_cached_index() {
    let origin = TestOrigin::start().await;
    origin.set("/index.json", Response::status(304));
    let svc = test_service().await;
    let id = seed_repo(&svc, &origin.base(), Some(INDEX_JSON)).await;

    svc.refresh_repo(id, None).await.unwrap();

    let cache: Option<String> =
        sqlx::query_scalar("SELECT index_cache FROM repo_trust WHERE id = ?")
            .bind(id)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert_eq!(
        cache.as_deref(),
        Some(INDEX_JSON),
        "the 304 reused the cached index unchanged"
    );
    let refreshed: Option<String> =
        sqlx::query_scalar("SELECT last_refreshed_at FROM repo_trust WHERE id = ?")
            .bind(id)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert!(refreshed.is_some(), "the refresh timestamp advanced");
}

// C8 — a 304 with no cached index is an error, not a silent empty index.
#[tokio::test]
async fn a_304_with_no_cached_index_is_an_error() {
    let origin = TestOrigin::start().await;
    origin.set("/index.json", Response::status(304));
    let svc = test_service().await;
    let id = seed_repo(&svc, &origin.base(), None).await;

    let res = svc.refresh_repo(id, None).await;

    assert!(
        res.is_err(),
        "a 304 with nothing cached must error rather than yield an empty index"
    );
}

// ── Group E — repo signature / failure handling ──────────────────────────────

use base64::Engine as _;

fn valid_pubkey_b64() -> String {
    use ed25519_dalek::SigningKey;
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    base64::engine::general_purpose::STANDARD.encode(sk.verifying_key().to_bytes())
}

// E6 — an index whose detached signature is missing (.sig 404) is refused, even
// though the index parses and its self-declared key matches the pinned one.
#[tokio::test]
async fn an_index_with_no_signature_is_refused() {
    let key = valid_pubkey_b64();
    let origin = TestOrigin::start().await;
    let index = format!(r#"{{"name":"Test Repo","maintainer_key":"{key}","extensions":[]}}"#);
    origin.set("/index.json", Response::json(&index));
    origin.set("/index.json.sig", Response::status(404));

    let svc = test_service().await;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO repo_trust (url, name, maintainer_key, index_cache) \
         VALUES (?, 'Test Repo', ?, NULL) RETURNING id",
    )
    .bind(origin.base())
    .bind(&key)
    .fetch_one(&svc.db)
    .await
    .unwrap();

    let res = svc.refresh_repo(id, None).await;
    assert!(
        res.is_err(),
        "an index served without its signature is refused, got {res:?}"
    );
}

// E7 — a repo that starts failing (index fetch 500) keeps its previously-cached
// index rather than clobbering it with an error/empty.
#[tokio::test]
async fn a_repo_that_starts_failing_does_not_lose_its_cached_index() {
    let origin = TestOrigin::start().await;
    origin.set("/index.json", Response::status(500));
    let svc = test_service().await;
    let id = seed_repo(&svc, &origin.base(), Some(INDEX_JSON)).await;

    let res = svc.refresh_repo(id, None).await;

    assert!(res.is_err(), "the failing refresh surfaces an error");
    let cache: Option<String> =
        sqlx::query_scalar("SELECT index_cache FROM repo_trust WHERE id = ?")
            .bind(id)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert_eq!(
        cache.as_deref(),
        Some(INDEX_JSON),
        "the cached index survived the failed refresh"
    );
}
