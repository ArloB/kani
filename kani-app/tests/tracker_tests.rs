#![allow(clippy::unwrap_used)]

//! Group L — tracker HTTP flows (MyAnimeList) driven against a TestOrigin via
//! the `with_test_base` seam, which also shortens the client timeout so a
//! stalled origin resolves quickly. Exercises the OAuth/refresh/status paths
//! that previously only ever hit the real provider.

use kani_app::service::trackers::ExternalTracker;
use kani_app::service::trackers::mal::MalTracker;
use kani_shared_test::origin::{Body, Response, TestOrigin};
use std::time::Duration;

fn mal(origin: &TestOrigin) -> MalTracker {
    MalTracker::new("test-client".into()).with_test_base(&origin.base())
}

// L8 — an OAuth code exchange that gets a provider error surfaces an error, not
// a silent success with an empty token.
#[tokio::test]
async fn the_oauth_code_exchange_surfaces_a_provider_error() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/token",
        Response::json(r#"{"error":"invalid_grant","error_description":"bad code"}"#),
    );

    let res = mal(&origin)
        .exchange_code("badcode", "http://localhost/cb", Some("verifier"))
        .await;
    assert!(
        res.is_err(),
        "a provider error body must surface as an error, not a token"
    );
}

// L5 — a stalled tracker does not hang the sync: the client timeout fires and
// the call returns an error promptly.
#[tokio::test]
async fn a_tracker_that_stalls_does_not_hang_the_sync_job() {
    let origin = TestOrigin::start().await;
    origin.set("/token", Response::status(200).body(Body::Stall));

    let start = std::time::Instant::now();
    let res = mal(&origin)
        .exchange_code("code", "http://localhost/cb", Some("v"))
        .await;
    let elapsed = start.elapsed();

    assert!(
        res.is_err(),
        "a stalled token endpoint must error, not hang"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the client timeout must bound the wait, took {elapsed:?}"
    );
}

// L6 — a malformed tracker response is a parse error, not silently-corrupted
// progress.
#[tokio::test]
async fn a_malformed_tracker_response_does_not_corrupt_progress() {
    let origin = TestOrigin::start().await;
    // get_status GETs {api}/manga/{id}; serve non-JSON garbage.
    origin.set("/manga/123", Response::html("<html>not json at all</html>"));

    let res = mal(&origin).get_status("token", "123").await;
    assert!(
        res.is_err(),
        "a non-JSON body must be a parse error, not a bogus status"
    );
}

// L1 (mechanism) — a token refresh returns the new credential pair the caller
// then persists. (The proactive-before-the-call wiring is the service-level
// get_valid_access_token path; this covers the client half.)
#[tokio::test]
async fn a_token_refresh_yields_the_new_credentials() {
    let origin = TestOrigin::start().await;
    origin.set(
        "/token",
        Response::json(
            r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":3600}"#,
        ),
    );

    let tokens = mal(&origin).refresh_token("old-refresh").await.unwrap();
    assert_eq!(
        tokens.access_token, "new-access",
        "the refreshed access token is returned"
    );
    assert_eq!(tokens.refresh_token.as_deref(), Some("new-refresh"));
}
