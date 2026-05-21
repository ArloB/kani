#![allow(clippy::unwrap_used)]
// Tests for /rest/trackers/* endpoints.
// Network-dependent operations (OAuth exchange, search, status push) require the
// tracker URL to be configurable; that is deferred to a later phase. These tests
// cover the DB-backed parts: listing tracker status and config get/set.

mod common;
use axum::http::StatusCode;
use common::{authed_get, body_array, body_json, build_test_app, create_admin, get_req, login, test_state};
use tower::ServiceExt;

#[tokio::test]
async fn list_trackers_returns_anilist_and_mal() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/trackers", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let trackers = body_array(res).await;
    assert_eq!(trackers.len(), 2, "AniList and MyAnimeList must always be seeded");
    let names: Vec<&str> = trackers
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"AniList"));
    assert!(names.contains(&"MyAnimeList"));
}

#[tokio::test]
async fn list_trackers_all_unconfigured_on_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/trackers", &cookie))
        .await
        .unwrap();

    let trackers = body_array(res).await;
    for tracker in &trackers {
        assert_eq!(
            tracker["configured"],
            serde_json::json!(false),
            "no tracker should be configured on a fresh DB"
        );
        assert_eq!(tracker["linked"], serde_json::json!(false));
    }
}

#[tokio::test]
async fn list_trackers_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/trackers"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_tracker_config_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Determine the AniList tracker id.
    let list_res = app
        .clone()
        .oneshot(authed_get("/rest/trackers", &cookie))
        .await
        .unwrap();
    let trackers = body_array(list_res).await;
    let anilist_id = trackers
        .iter()
        .find(|t| t["name"].as_str() == Some("AniList"))
        .and_then(|t| t["id"].as_i64())
        .expect("AniList tracker must exist");

    let res = app
        .oneshot(authed_get(
            &format!("/rest/trackers/{anilist_id}/config"),
            &cookie,
        ))
        .await
        .unwrap();

    // No config stored → 200 with {"client_id": null, "secret_configured": false}.
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["secret_configured"], serde_json::json!(false));
}

#[tokio::test]
async fn get_tracker_config_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(get_req("/rest/trackers/1/config"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
