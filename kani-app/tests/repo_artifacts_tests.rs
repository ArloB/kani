#![allow(clippy::unwrap_used)]

//! Group E — repo artifact integrity on install. `install_source_from_repo` reads
//! the (already-trusted) cached index, then downloads the artifact and checks its
//! size, sha256 and signature in that order. E1/E2/E5 exercise the rejections that
//! fire *before* the signature check, so no real signing is needed — the index is
//! seeded directly into `repo_trust`.

mod common;
use common::test_service;
use kani_app::error::ServiceError;
use kani_app::service::AppService;
use kani_shared_test::origin::{Body, Response, TestOrigin};

/// Seed a repo whose cached index advertises one extension `ext1` at `artifact_url`
/// with the given sha256 (author_key/signature are dummies — E1/E2/E5 never reach
/// the signature check).
async fn seed_repo_with_entry(
    svc: &AppService,
    repo_url: &str,
    artifact_url: &str,
    sha256: &str,
) -> i64 {
    let index = serde_json::json!({
        "name": "Test Repo",
        "maintainer_key": "KEY",
        "extensions": [{
            "id": "ext1",
            "name": "Ext One",
            "version": "1.0.0",
            "format": "wasm",
            "sha256": sha256,
            "signature": "x",
            "author_key": "x",
            "url": artifact_url,
        }],
    })
    .to_string();
    sqlx::query_scalar(
        "INSERT INTO repo_trust (url, name, maintainer_key, index_cache) \
         VALUES (?, 'Test Repo', 'KEY', ?) RETURNING id",
    )
    .bind(repo_url)
    .bind(index)
    .fetch_one(&svc.db)
    .await
    .unwrap()
}

// E1 — an artifact whose declared size is past the cap is rejected before it is
// downloaded (the cap is checked against Content-Length).
#[tokio::test]
async fn an_artifact_larger_than_the_cap_is_rejected() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/ext1.wasm",
        Response::status(200)
            .header("Content-Type", "application/wasm")
            .body(Body::Truncated {
                bytes: vec![0u8; 16],
                announced: 50 * 1024 * 1024, // > MAX_ARTIFACT_BYTES (10 MB)
                sent: 16,
            }),
    );
    let svc = test_service().await;
    let repo_id = seed_repo_with_entry(&svc, &origin.base(), "/ext1.wasm", "deadbeef").await;

    let res = svc.install_source_from_repo(repo_id, "ext1", None).await;

    assert!(
        matches!(res, Err(ServiceError::Internal(_))),
        "an oversized artifact is rejected, got {res:?}"
    );
}

// E2 — an artifact whose bytes do not match the index's sha256 is rejected.
#[tokio::test]
async fn an_artifact_whose_hash_does_not_match_is_rejected() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/ext1.wasm",
        Response::status(200)
            .header("Content-Type", "application/wasm")
            .body(Body::Bytes(b"the real artifact bytes".to_vec())),
    );
    let svc = test_service().await;
    // sha256 the index claims is not the sha256 of what the origin serves.
    let repo_id = seed_repo_with_entry(&svc, &origin.base(), "/ext1.wasm", &"00".repeat(32)).await;

    let res = svc.install_source_from_repo(repo_id, "ext1", None).await;

    match res {
        Err(ServiceError::Validation(msg)) => assert!(
            msg.contains("Integrity"),
            "hash mismatch surfaces as an integrity failure, got: {msg}"
        ),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

// E5 — redirect bounding during the artifact download is covered by the SmartClient
// unit test `safe_get_too_many_redirects_returns_error` (install just calls
// `safe_get`). A TestOrigin-based duplicate here hit a harness quirk (a redirect
// *loop* hangs to the 35s client timeout instead of tripping MAX_REDIRECTS fast),
// tracked separately; not re-asserted through the slow path.
