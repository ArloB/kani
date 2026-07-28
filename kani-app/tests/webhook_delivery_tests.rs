#![allow(clippy::unwrap_used)]

//! Group M — webhook delivery. `create_webhook` refuses loopback URLs (SSRF
//! guard, covered by webhook_ssrf_tests), so delivery is exercised by inserting
//! the row directly and opting the egress guard into private hosts — the same
//! pattern jobs_tests uses for the delivery job.

mod common;
use common::test_service;

use hmac::{Hmac, Mac};
use kani_shared_test::origin::{Response, TestOrigin};
use sha2::Sha256;

// M1 — a delivery records the upstream status it actually got in
// webhook_deliveries: a 200 then a 500 land as their real codes.
#[tokio::test]
async fn a_webhook_delivery_records_its_status() {
    let svc = test_service().await;
    svc.webhook_service.allow_private_egress_for_test();
    let origin = TestOrigin::start().await;

    let webhook_id: i64 =
        sqlx::query_scalar("INSERT INTO webhooks (url, enabled) VALUES (?, 1) RETURNING id")
            .bind(origin.url("/hook"))
            .fetch_one(&svc.db)
            .await
            .unwrap();

    // A healthy delivery records 200.
    origin.set("/hook", Response::json("{}"));
    let (status, err) = svc
        .webhook_service
        .send_signed(&origin.url("/hook"), None, "{}")
        .await;
    svc.webhook_service
        .record_delivery(webhook_id, "chapter.new", "{}", status, err)
        .await;

    // A failing delivery records the upstream 500 (a 500 is still an HTTP
    // response, not a transport error).
    origin.set("/hook", Response::status(500));
    let (status2, err2) = svc
        .webhook_service
        .send_signed(&origin.url("/hook"), None, "{}")
        .await;
    svc.webhook_service
        .record_delivery(webhook_id, "chapter.new", "{}", status2, err2)
        .await;

    let statuses: Vec<Option<i64>> = sqlx::query_scalar(
        "SELECT http_status FROM webhook_deliveries WHERE webhook_id = ? ORDER BY id",
    )
    .bind(webhook_id)
    .fetch_all(&svc.db)
    .await
    .unwrap();
    assert_eq!(
        statuses,
        vec![Some(200), Some(500)],
        "each delivery records the actual upstream status"
    );
}

// M4 — the X-Kani-Signature header is HMAC-SHA256 of the exact delivered body,
// keyed by the webhook secret. Recompute it from what actually arrived.
#[tokio::test]
async fn the_hmac_signature_matches_the_delivered_body() {
    let svc = test_service().await;
    svc.webhook_service.allow_private_egress_for_test();
    let origin = TestOrigin::start().await;
    origin.set("/hook", Response::json("{}"));

    let body = r#"{"event":"chapter.new","id":42}"#;
    let secret = "s3cr3t-key";
    let (status, err) = svc
        .webhook_service
        .send_signed(&origin.url("/hook"), Some(secret), body)
        .await;
    assert_eq!(status, Some(200));
    assert!(err.is_none());

    let seen = origin
        .last_request("/hook")
        .expect("the webhook was delivered");
    let sig = seen
        .header("x-kani-signature")
        .expect("the signature header is present");

    let mut mac = <Hmac<Sha256>>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body.as_bytes());
    let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    assert_eq!(
        sig, expected,
        "X-Kani-Signature must be HMAC-SHA256 of the exact delivered body"
    );
}

// M4b — with no secret configured, no signature header is attached.
#[tokio::test]
async fn an_unsigned_webhook_carries_no_signature_header() {
    let svc = test_service().await;
    svc.webhook_service.allow_private_egress_for_test();
    let origin = TestOrigin::start().await;
    origin.set("/hook", Response::json("{}"));

    svc.webhook_service
        .send_signed(&origin.url("/hook"), None, "{}")
        .await;

    let seen = origin.last_request("/hook").expect("delivered");
    assert!(
        seen.header("x-kani-signature").is_none(),
        "a webhook without a secret must not carry a signature header"
    );
}
