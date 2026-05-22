#![allow(clippy::unwrap_used)]
// Tests cover DB-level tracker operations: credential storage, PKCE state,
// mappings, config management, and unlink.
// Network-dependent operations (search, status push, OAuth exchange) require
// the tracker URL to be overridable; that refactor is deferred to a later phase.

mod common;
use common::{insert_manga, insert_source, insert_user, test_service};
use kani_app::service::trackers::{
    TokenResponse, consume_pkce_state, delete_mapping, get_mapping, set_mapping, store_credentials,
    store_pkce_state,
};

/// Look up the DB id of a seeded tracker by name.
async fn tracker_id(svc: &kani_app::AppService, name: &str) -> i64 {
    sqlx::query_scalar("SELECT id FROM trackers WHERE name = ?")
        .bind(name)
        .fetch_one(&svc.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn list_trackers_status_returns_anilist_and_mal_unconfigured() {
    let svc = test_service().await;
    let items = svc.list_trackers_status(1).await.unwrap();
    assert_eq!(
        items.len(),
        2,
        "AniList and MyAnimeList should always be seeded"
    );
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"AniList"));
    assert!(names.contains(&"MyAnimeList"));
    // Without env vars or DB config, none are configured or linked.
    assert!(items.iter().all(|i| !i.configured));
    assert!(items.iter().all(|i| !i.linked));
}

#[tokio::test]
async fn pkce_state_store_and_consume_is_single_use() {
    let svc = test_service().await;
    let tid = tracker_id(&svc, "AniList").await;

    store_pkce_state(
        &svc.db,
        "csrf-abc",
        Some("verifier-xyz"),
        tid,
        "https://app/cb",
    )
    .await
    .unwrap();

    let pkce = consume_pkce_state(&svc.db, "csrf-abc").await.unwrap();
    assert!(pkce.is_some());
    let pkce = pkce.unwrap();
    assert_eq!(pkce.code_verifier.as_deref(), Some("verifier-xyz"));
    assert_eq!(pkce.tracker_id, tid);
    assert_eq!(pkce.redirect_uri, "https://app/cb");

    // Consuming again must return None — the state is single-use.
    let again = consume_pkce_state(&svc.db, "csrf-abc").await.unwrap();
    assert!(again.is_none());
}

#[tokio::test]
async fn pkce_state_unknown_state_returns_none() {
    let svc = test_service().await;
    let result = consume_pkce_state(&svc.db, "no-such-state").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn credential_store_persists_access_token() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "alice").await;
    let tid = tracker_id(&svc, "AniList").await;

    let tokens = TokenResponse {
        access_token: "my-access-token".to_string(),
        refresh_token: Some("my-refresh-token".to_string()),
        expires_at: None,
    };
    store_credentials(&svc.db, user_id, tid, &tokens, None)
        .await
        .unwrap();

    let stored: String = sqlx::query_scalar(
        "SELECT access_token FROM user_tracker_credentials WHERE user_id = ? AND tracker_id = ?",
    )
    .bind(user_id)
    .bind(tid)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(stored, "my-access-token");

    // list_trackers_status should now show linked = true for this tracker.
    let items = svc.list_trackers_status(user_id).await.unwrap();
    let anilist = items.iter().find(|i| i.name == "AniList").unwrap();
    assert!(anilist.linked);
}

#[tokio::test]
async fn mapping_set_get_delete_round_trips() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "bob").await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Dragon Ball").await;
    let tid = tracker_id(&svc, "AniList").await;

    // No mapping initially.
    let none = get_mapping(&svc.db, user_id, tid, manga_id).await.unwrap();
    assert!(none.is_none());

    // Set → get.
    set_mapping(&svc.db, user_id, tid, manga_id, "anilist-123")
        .await
        .unwrap();
    let found = get_mapping(&svc.db, user_id, tid, manga_id).await.unwrap();
    assert_eq!(found.as_deref(), Some("anilist-123"));

    // Overwrite via upsert.
    set_mapping(&svc.db, user_id, tid, manga_id, "anilist-456")
        .await
        .unwrap();
    let updated = get_mapping(&svc.db, user_id, tid, manga_id).await.unwrap();
    assert_eq!(updated.as_deref(), Some("anilist-456"));

    // Delete → None.
    delete_mapping(&svc.db, user_id, tid, manga_id)
        .await
        .unwrap();
    let gone = get_mapping(&svc.db, user_id, tid, manga_id).await.unwrap();
    assert!(gone.is_none());
}

#[tokio::test]
async fn unlink_tracker_removes_credentials_and_mappings() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "carol").await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Naruto").await;
    let tid = tracker_id(&svc, "AniList").await;

    let tokens = TokenResponse {
        access_token: "tok".to_string(),
        refresh_token: None,
        expires_at: None,
    };
    store_credentials(&svc.db, user_id, tid, &tokens, None)
        .await
        .unwrap();
    set_mapping(&svc.db, user_id, tid, manga_id, "remote-id")
        .await
        .unwrap();

    svc.unlink_tracker(user_id, tid).await.unwrap();

    let cred: Option<String> = sqlx::query_scalar(
        "SELECT access_token FROM user_tracker_credentials WHERE user_id = ? AND tracker_id = ?",
    )
    .bind(user_id)
    .bind(tid)
    .fetch_optional(&svc.db)
    .await
    .unwrap();
    assert!(cred.is_none(), "credentials must be removed after unlink");

    let mapping = get_mapping(&svc.db, user_id, tid, manga_id).await.unwrap();
    assert!(mapping.is_none(), "mapping must be removed after unlink");
}

#[tokio::test]
async fn set_and_get_tracker_config_round_trips() {
    let svc = test_service().await;
    let tid = tracker_id(&svc, "AniList").await;

    // Initially no config.
    let none = svc.get_tracker_config(tid).await.unwrap();
    assert!(none.is_none());

    svc.set_tracker_config(tid, "client-id-123", Some("client-secret-abc"))
        .await
        .unwrap();

    let config = svc.get_tracker_config(tid).await.unwrap();
    assert!(config.is_some());
    let (client_id, has_secret) = config.unwrap();
    assert_eq!(client_id, "client-id-123");
    assert!(has_secret);

    // After config, the tracker should appear as configured.
    let items = svc.list_trackers_status(1).await.unwrap();
    let anilist = items.iter().find(|i| i.name == "AniList").unwrap();
    assert!(anilist.configured);
}
