#![allow(clippy::unwrap_used)]
// Tests for /rest/settings: GET and PATCH round-trips.

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_patch, body_json, build_test_app, create_admin, get_req, login, test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn get_settings_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/settings", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    // Settings object always contains auto_scan.
    assert!(
        body.get("auto_scan").is_some(),
        "settings must include auto_scan"
    );
}

#[tokio::test]
async fn get_settings_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/settings")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn patch_settings_scan_updates_interval() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_patch(
            "/rest/settings",
            &cookie,
            serde_json::json!({
                "Scan": {
                    "auto_scan": false,
                    "scan_interval_minutes": 120,
                    "scan_exclude_completed": false
                }
            }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);

    let get_res = app
        .oneshot(authed_get("/rest/settings", &cookie))
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);
    let body = body_json(get_res).await;
    assert_eq!(body["scan_interval_minutes"], serde_json::json!(120));
}

#[tokio::test]
async fn patch_settings_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            "",
            serde_json::json!({"Scan": {"auto_scan": false, "scan_interval_minutes": 60, "scan_exclude_completed": false}}),
        ))
        .await
        .unwrap();

    // auth_guard rejects the request (no valid session).
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn patch_settings_invalid_body_returns_4xx() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    // Wrong type for auto_scan (string instead of bool) → JSON deserialize error.
    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            &cookie,
            serde_json::json!({"Scan": {"auto_scan": "not-a-bool", "scan_interval_minutes": 60, "scan_exclude_completed": false}}),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "invalid settings body should return 4xx, got {}",
        res.status()
    );
}
