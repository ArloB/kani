#![allow(clippy::unwrap_used)]

//! Group C — the SmartClient circuit breaker and transport-error retry, driven by
//! real origin responses for the first time (previously only ever hand-driven via
//! `record_failure`). `Timings` shortens the retry backoff/jitter and the circuit
//! cooldown so these exercise the production code paths in milliseconds.

use kani_core::http::{SmartClient, Timings};
use kani_shared_test::origin::{Body, Response, TestOrigin};
use std::time::Duration;

/// Near-instant retries so the real retry/circuit loop runs without minute-long
/// waits. `cooldown` is separated out because C10 needs a specific short value.
fn fast_timings(cooldown: Duration) -> Timings {
    Timings {
        retry_base_delay: Duration::from_millis(1),
        retry_jitter: Duration::ZERO,
        circuit_cooldown: cooldown,
        ..Timings::default()
    }
}

fn fast_client() -> SmartClient {
    SmartClient::new(None)
        .unwrap()
        .with_timings(fast_timings(Duration::from_secs(30)))
}

// C9 — after the failure threshold, the circuit opens and further calls are
// rejected before a socket is opened.
#[tokio::test]
async fn the_circuit_opens_after_repeated_real_failures() {
    let site = TestOrigin::start().await;
    site.set("/x", Response::status(502));
    let client = fast_client();

    // Five 502 responses → five recorded failures → the circuit opens (threshold 5).
    for _ in 0..5 {
        let _ = client.get(&site.url("/x")).await;
    }

    let hits_before = site.hits("/x");
    let res = client.get(&site.url("/x")).await;

    assert!(res.is_err(), "an open circuit rejects the call");
    assert_eq!(
        site.hits("/x"),
        hits_before,
        "an open circuit never reaches the socket"
    );
}

// C10 — once the cooldown elapses the circuit lets a call through again, and a
// success resets it.
#[tokio::test]
async fn the_circuit_recovers_after_the_cooldown() {
    let site = TestOrigin::start().await;
    site.set("/x", Response::status(502));
    let client = SmartClient::new(None)
        .unwrap()
        .with_timings(fast_timings(Duration::from_millis(50)));

    for _ in 0..5 {
        let _ = client.get(&site.url("/x")).await;
    }
    assert!(
        client.get(&site.url("/x")).await.is_err(),
        "the circuit is open immediately after the threshold"
    );

    // Recover the origin, then wait out the (shortened) cooldown.
    site.set("/x", Response::html("<html>ok</html>"));
    tokio::time::sleep(Duration::from_millis(120)).await;

    let status = client
        .get(&site.url("/x"))
        .await
        .expect("after the cooldown the circuit half-opens and the call goes through")
        .status();
    assert_eq!(status.as_u16(), 200);
}

// C11 — a connection reset mid-request is retried rather than surfaced.
#[tokio::test]
async fn a_connection_reset_mid_request_is_retried() {
    let site = TestOrigin::start().await;
    site.script(
        "/x",
        vec![
            Response::status(200).body(Body::Reset),
            Response::status(200).body(Body::Reset),
            Response::html("<html>ok</html>"),
        ],
    );
    let client = fast_client();

    let status = client
        .get(&site.url("/x"))
        .await
        .expect("two resets then a success must be retried through")
        .status();

    assert_eq!(status.as_u16(), 200);
    assert_eq!(site.hits("/x"), 3, "each reset attempt reached the socket");
}
