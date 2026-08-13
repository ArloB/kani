#![allow(clippy::unwrap_used)]

//! A live probe for the managed-challenge fallback in
//! `capture_page_payload_resilient`. Mock challenges cannot validate browser
//! fingerprint behaviour, so this drives the real path: Puppeteer navigates,
//! Cloudflare blocks it, and the capture is retried inside the solver's own
//! cleared browser via `kani.capture`.
//!
//! Ignored by default — it needs network, a Chromium, and a running
//! Kani-compatible FlareSolverr. Run it with:
//!
//! ```text
//! KANI_LIVE_URL='https://comix.to/browse?page=1&sort=score%3Adesc' \
//! KANI_LIVE_INIT_SCRIPT_FILE=/path/to/capture_browse.js \
//! KANI_LIVE_SOLVER_URL=http://127.0.0.1:8191/v1 \
//! CHROMIUM_PATH=/path/to/chrome \
//! KANI_PUPPETEER_MODULE=/path/to/node_modules/puppeteer-core \
//! cargo test -p kani-app --test live_browser_capture_tests -- --ignored --nocapture
//! ```

use kani_core::http::SmartClient;
use kani_core::v8_process;

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for the live probe"))
}

#[tokio::test]
#[ignore = "live probe: needs network, Chromium, and a Kani-compatible solver"]
async fn a_managed_challenge_falls_back_to_the_solver_and_yields_a_payload() {
    let url = required("KANI_LIVE_URL");
    let script_file = required("KANI_LIVE_INIT_SCRIPT_FILE");
    let solver_url = required("KANI_LIVE_SOLVER_URL");
    let init_script = std::fs::read_to_string(&script_file).unwrap();

    let handle = v8_process::new_handle();
    let http = SmartClient::new(Some(solver_url)).unwrap();

    let first_started = std::time::Instant::now();
    let payload = v8_process::capture_page_payload_resilient(
        &handle,
        &http,
        &url,
        &init_script,
        45_000,
        Some("live-probe"),
        false,
    )
    .await
    .expect("the challenged page must still yield a payload via the solver");

    println!(
        "cold capture: {} bytes in {:?}",
        payload.len(),
        first_started.elapsed()
    );
    assert!(!payload.is_empty(), "the capture returned an empty payload");
    serde_json::from_str::<serde_json::Value>(&payload)
        .expect("the captured payload must be the site's JSON, not a challenge page");

    let warm_started = std::time::Instant::now();
    let warm_payload = v8_process::capture_page_payload_resilient(
        &handle,
        &http,
        &url,
        &init_script,
        45_000,
        Some("live-probe"),
        false,
    )
    .await
    .expect("the cleared solver session must capture a second payload");
    println!(
        "warm capture: {} bytes in {:?}",
        warm_payload.len(),
        warm_started.elapsed()
    );
    serde_json::from_str::<serde_json::Value>(&warm_payload)
        .expect("the warm capture must return the site's JSON");

    assert_eq!(http.destroy_solver_sessions("live-probe").await, 1);
}

/// Cell D of the Puppeteer-removal measurement: warm Puppeteer capture latency
/// against a local unchallenged fixture. The solver is measured against the
/// same fixture (cell C) so the two are comparable — pitting warm Puppeteer on
/// a fixture against warm solver on Comix would confound engine with site.
///
/// Emits `BENCH <index> <cold|warm> <elapsed_ms> <bytes>` for the harness.
#[tokio::test]
#[ignore = "bench: needs a local fixture server and a Chromium"]
async fn puppeteer_capture_latency_against_a_local_fixture() {
    let url = required("KANI_BENCH_FIXTURE_URL");
    let init_script = std::fs::read_to_string(required("KANI_BENCH_FIXTURE_SCRIPT_FILE")).unwrap();
    let iterations: usize = std::env::var("KANI_BENCH_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let handle = v8_process::new_handle();

    for index in 0..=iterations {
        let started = std::time::Instant::now();
        let outcome = v8_process::capture_page_payload_detailed(
            &handle,
            &url,
            &init_script,
            15_000,
            Some("bench-fixture"),
            false,
            None,
        )
        .await;
        let elapsed = started.elapsed().as_millis();
        let phase = if index == 0 { "cold" } else { "warm" };
        match outcome {
            Ok(payload) => println!("BENCH {index} {phase} {elapsed} {}", payload.len()),
            Err(error) => println!("BENCH {index} {phase} {elapsed} FAILED {error}"),
        }
    }
}
