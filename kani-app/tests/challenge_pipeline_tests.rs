#![allow(clippy::unwrap_used)]

//! Group C — the challenge/FlareSolverr fallback in `kani_core::http::SmartClient`,
//! previously unexercised (no test ever set a `solver_url`). A `TestOrigin` plays
//! the protected site and a second one plays the fake solver, returning the
//! FlareSolverr envelope. The 503/403 paths are used deliberately: neither status
//! is retryable, so these never hit the 5 s×2^n retry backoff.

use kani_core::http::SmartClient;
use kani_shared_test::origin::{Response, TestOrigin};

/// A FlareSolverr `request.get` success envelope. `rendered` is echoed back as
/// `solution.response` (the HTML the solver "saw" after clearing the challenge).
fn solver_envelope(rendered: &str) -> String {
    serde_json::json!({
        "status": "ok",
        "solution": {
            "userAgent": "FlareUA/1.0",
            "response": rendered,
            "cookies": [{ "name": "cf_clearance", "value": "TOKEN123" }],
        },
    })
    .to_string()
}

// C1 — an interstitial "Just a moment…" HTML page routes through the solver and
// the caller receives the rendered page, not the challenge.
#[tokio::test]
async fn a_challenge_page_triggers_the_solver_and_replays() {
    let site = TestOrigin::start().await;
    site.set(
        "/page",
        Response::html("<html><body>Just a moment...</body></html>"),
    );
    let solver = TestOrigin::start().await;
    solver.set(
        "/v1",
        Response::json(&solver_envelope("<html><body>REAL CONTENT</body></html>")),
    );

    let client = SmartClient::new(Some(solver.url("/v1"))).unwrap();
    let resp = client.get(&site.url("/page")).await.unwrap();
    let body = resp.text().await.unwrap();

    assert!(
        body.contains("REAL CONTENT"),
        "the caller gets the solver-rendered page, got: {body}"
    );
    assert!(solver.hits("/v1") >= 1, "the solver was invoked");
}

// C2 — after a 403/503 solve, the solved cookie rides the replayed request.
#[tokio::test]
async fn the_solved_cookie_is_attached_to_the_replay() {
    let site = TestOrigin::start().await;
    site.script(
        "/page",
        vec![Response::status(503), Response::html("<html>ok</html>")],
    );
    let solver = TestOrigin::start().await;
    solver.set("/v1", Response::json(&solver_envelope("unused")));

    let client = SmartClient::new(Some(solver.url("/v1"))).unwrap();
    let resp = client.get(&site.url("/page")).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let replay = site.last_request("/page").unwrap();
    let cookie = replay.header("cookie").unwrap_or("");
    assert!(
        cookie.contains("cf_clearance=TOKEN123"),
        "the replay carried the solved cookie, got: {cookie:?}"
    );
}

// C3 — a solver that reports an error surfaces a real error, not a silent pass.
#[tokio::test]
async fn a_solver_error_status_surfaces_as_a_useful_error() {
    let site = TestOrigin::start().await;
    site.set("/page", Response::status(503));
    let solver = TestOrigin::start().await;
    solver.set(
        "/v1",
        Response::json(r#"{"status":"error","message":"challenge-boom"}"#),
    );

    let client = SmartClient::new(Some(solver.url("/v1"))).unwrap();
    let msg = match client.get(&site.url("/page")).await {
        Ok(_) => panic!("expected the solver error to surface, got a response"),
        Err(e) => e.to_string(),
    };

    assert!(
        msg.contains("challenge-boom") || msg.to_lowercase().contains("flaresolverr"),
        "the solver error is surfaced, got: {msg}"
    );
}

// C5 — a 403 after a prior solve clears the stored credential and re-solves,
// rather than replaying a stale cookie forever.
#[tokio::test]
async fn stored_credentials_are_re_solved_after_a_403() {
    let site = TestOrigin::start().await;
    site.script(
        "/page",
        vec![
            Response::status(403),
            Response::html("<html>ok1</html>"),
            Response::status(403),
            Response::html("<html>ok2</html>"),
        ],
    );
    let solver = TestOrigin::start().await;
    solver.set("/v1", Response::json(&solver_envelope("unused")));

    let client = SmartClient::new(Some(solver.url("/v1"))).unwrap();
    assert_eq!(
        client
            .get(&site.url("/page"))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );
    assert_eq!(
        client
            .get(&site.url("/page"))
            .await
            .unwrap()
            .status()
            .as_u16(),
        200
    );

    assert_eq!(
        solver.hits("/v1"),
        2,
        "the second 403 dropped the stored credential and forced a fresh solve"
    );
}

// Baseline — with no solver configured, a challenge is passed through untouched
// (the guard that gates the whole path on `solver_url`).
#[tokio::test]
async fn without_a_solver_a_challenge_is_passed_through() {
    let site = TestOrigin::start().await;
    site.set("/page", Response::status(503));

    let client = SmartClient::new(None).unwrap();
    let resp = client.get(&site.url("/page")).await.unwrap();

    assert_eq!(
        resp.status().as_u16(),
        503,
        "the raw challenge status is returned"
    );
}
