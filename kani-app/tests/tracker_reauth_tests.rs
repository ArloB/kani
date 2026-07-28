#![allow(clippy::unwrap_used)]

//! Group L2/L3 — token-failure recovery. Two defects the plan flagged:
//!
//! * **L3** — a refresh that the provider rejects (`400 invalid_grant`) just
//!   propagates its error; nothing records that the link is dead, so every
//!   later sync retries the same doomed refresh and logs a warning forever.
//! * **L2** — nothing reacts to a `401`. A token revoked *before* its
//!   `expires_at` is never refreshed (the refresh is proactive-only) and never
//!   flagged, so the link silently stops working.
//!
//! Both are fixed by recording `needs_reauth` on the credential row, which the
//! trackers settings page surfaces so the user knows to re-link.

mod common;
use common::{insert_user, test_service};

use kani_app::ids::UserId;
use kani_app::service::trackers::mal::MalTracker;
use kani_app::service::trackers::{ExternalTracker, get_access_token};
use kani_shared_test::origin::{Response, TestOrigin};
use sqlx::SqlitePool;

/// `TrackerRegistry::new` seeds the provider rows, so look the id up rather
/// than inserting a colliding one.
async fn tracker_id(db: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT id FROM trackers WHERE name = 'MyAnimeList'")
        .fetch_one(db)
        .await
        .unwrap()
}

/// Credentials whose `expires_at` is `offset_secs` from now (negative = expired).
async fn seed_credentials(db: &SqlitePool, user: UserId, tid: i64, offset_secs: i64) {
    let expires = (time::OffsetDateTime::now_utc() + time::Duration::seconds(offset_secs))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    sqlx::query(
        "INSERT INTO user_tracker_credentials \
         (user_id, tracker_id, access_token, refresh_token, expires_at) \
         VALUES (?, ?, 'stored-access', 'stored-refresh', ?)",
    )
    .bind(user.0)
    .bind(tid)
    .bind(expires)
    .execute(db)
    .await
    .unwrap();
}

async fn needs_reauth(db: &SqlitePool, user: UserId, tid: i64) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT needs_reauth FROM user_tracker_credentials WHERE user_id = ? AND tracker_id = ?",
    )
    .bind(user.0)
    .bind(tid)
    .fetch_one(db)
    .await
    .unwrap()
}

// L3 — a refresh the provider rejects marks the link as needing reauth instead
// of failing forever in silence.
#[tokio::test]
async fn a_failed_refresh_marks_the_link_as_needing_reauth() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc.db).await;
    // Already expired, so get_access_token attempts the proactive refresh.
    seed_credentials(&svc.db, user, tid, -60).await;

    let origin = TestOrigin::start().await;
    origin.set(
        "/token",
        Response::json(r#"{"error":"invalid_grant","error_description":"expired"}"#),
    );
    let tracker = MalTracker::new("client".into()).with_test_base(&origin.base());

    let res = get_access_token(&svc.db, tid, user, &tracker, None).await;
    assert!(res.is_err(), "a rejected refresh must surface an error");

    assert!(
        needs_reauth(&svc.db, user, tid).await,
        "a rejected refresh must mark the link as needing reauth — otherwise every \
         later sync retries the same doomed refresh forever"
    );
}

// L1 — an expired token is refreshed *before* the call, and the refreshed
// credentials are what the caller gets. This never worked: `expires_at` was
// decoded into a `time` value whose `to_string()` is not RFC3339, so the
// re-parse always failed and `needs_refresh` was permanently false.
#[tokio::test]
async fn an_expired_token_is_refreshed_before_the_call() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc.db).await;
    seed_credentials(&svc.db, user, tid, -60).await;

    let origin = TestOrigin::start().await;
    origin.set(
        "/token",
        Response::json(
            r#"{"access_token":"refreshed-access","refresh_token":"r2","expires_in":3600}"#,
        ),
    );
    let tracker = MalTracker::new("client".into()).with_test_base(&origin.base());

    let token = get_access_token(&svc.db, tid, user, &tracker, None)
        .await
        .unwrap();

    assert_eq!(
        token, "refreshed-access",
        "an expired token must be refreshed before use, not handed back stale"
    );
    assert_eq!(
        origin.hits("/token"),
        1,
        "the refresh must actually reach the provider"
    );
}

// A token that is still valid must NOT trigger a refresh.
#[tokio::test]
async fn a_valid_token_is_used_without_refreshing() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc.db).await;
    seed_credentials(&svc.db, user, tid, 3600).await;

    let origin = TestOrigin::start().await;
    origin.set("/token", Response::json(r#"{"access_token":"nope"}"#));
    let tracker = MalTracker::new("client".into()).with_test_base(&origin.base());

    let token = get_access_token(&svc.db, tid, user, &tracker, None)
        .await
        .unwrap();

    assert_eq!(token, "stored-access", "a live token is used as-is");
    assert_eq!(origin.hits("/token"), 0, "no refresh for a valid token");
}

// L2 — a 401 from the API (token revoked while still notionally valid) is
// recognised as an auth failure rather than a generic parse error.
#[tokio::test]
async fn a_revoked_token_is_reported_as_an_auth_failure() {
    let origin = TestOrigin::start().await;
    origin.set("/manga/123", Response::status(401));
    let tracker = MalTracker::new("client".into()).with_test_base(&origin.base());

    let err = tracker
        .get_status("revoked-token", "123")
        .await
        .expect_err("a 401 must be an error");

    assert!(
        matches!(err, kani_app::error::ServiceError::TrackerAuthExpired(_)),
        "a 401 must be a distinguishable auth failure, not a generic parse error — \
         got {err:?}"
    );
}

// L2 — and the reactive recovery: syncing with a revoked token flags the link.
#[tokio::test]
async fn a_revoked_token_is_recovered_from_reactively() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc.db).await;
    // NOT expired — the proactive refresh never fires, so only a reactive 401
    // check can notice the token is dead.
    seed_credentials(&svc.db, user, tid, 3600).await;

    let origin = TestOrigin::start().await;
    origin.set("/manga/123", Response::status(401));
    {
        let mut registry = svc.tracker_registry.write().await;
        registry.trackers.insert(
            tid,
            Box::new(MalTracker::new("client".into()).with_test_base(&origin.base())),
        );
    }

    let source = common::insert_source(&svc.db, "src").await;
    let manga = common::insert_manga(&svc.db, source, "m1", "Tracked").await;
    svc.set_manga_tracking_enabled(user, manga, true)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO tracker_manga_mappings (user_id, tracker_id, manga_id, tracker_manga_id) \
         VALUES (?, ?, ?, '123')",
    )
    .bind(user.0)
    .bind(tid)
    .bind(manga.0)
    .execute(&svc.db)
    .await
    .unwrap();

    let _ = svc.sync_manga_trackers(user, manga).await;

    assert!(
        needs_reauth(&svc.db, user, tid).await,
        "a 401 during sync must flag the link for re-authentication"
    );
}

// Re-linking clears the flag — otherwise the warning would stick forever.
#[tokio::test]
async fn storing_fresh_credentials_clears_the_reauth_flag() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc.db).await;
    seed_credentials(&svc.db, user, tid, -60).await;
    sqlx::query("UPDATE user_tracker_credentials SET needs_reauth = 1 WHERE user_id = ?")
        .bind(user.0)
        .execute(&svc.db)
        .await
        .unwrap();

    kani_app::service::trackers::store_credentials(
        &svc.db,
        user,
        tid,
        &kani_app::service::trackers::TokenResponse {
            access_token: "fresh".into(),
            refresh_token: Some("fresh-refresh".into()),
            expires_at: None,
        },
        None,
    )
    .await
    .unwrap();

    assert!(
        !needs_reauth(&svc.db, user, tid).await,
        "re-linking must clear the needs-reauth flag"
    );
}
