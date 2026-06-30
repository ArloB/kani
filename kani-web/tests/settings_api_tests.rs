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

#[tokio::test]
async fn get_settings_shows_env_setting_defaults() {
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
    assert_eq!(body["trash_retention_days"], serde_json::json!(30));
    assert_eq!(body["audit_retention_days"], serde_json::json!(365));
    assert_eq!(body["audit_security_retention_days"], serde_json::json!(0));
    assert_eq!(body["max_login_attempts"], serde_json::json!(5));
    assert_eq!(body["max_ip_attempts"], serde_json::json!(20));
    assert_eq!(body["login_lockout_seconds"], serde_json::json!(900));
    assert_eq!(body["session_timeout_secs"], serde_json::json!(2592000));
    assert_eq!(body["thumbnail_formats"], serde_json::json!("jpeg"));
}

#[tokio::test]
async fn patch_settings_maintenance_updates() {
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
                "Maintenance": {
                    "trash_retention_days": 14,
                    "audit_retention_days": 180,
                    "audit_security_retention_days": 90,
                    "disk_warn_threshold": 0.25,
                    "thumbnail_formats": "jpeg"
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
    let body = body_json(get_res).await;
    assert_eq!(body["trash_retention_days"], serde_json::json!(14));
    assert_eq!(body["audit_retention_days"], serde_json::json!(180));
    assert_eq!(body["audit_security_retention_days"], serde_json::json!(90));
    assert_eq!(body["disk_warn_threshold"], serde_json::json!(0.25));
}

#[tokio::test]
async fn patch_settings_security_updates() {
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
                "Security": {
                    "max_login_attempts": 8,
                    "max_ip_attempts": 40,
                    "login_lockout_seconds": 600,
                    "session_timeout_secs": 86400
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
    let body = body_json(get_res).await;
    assert_eq!(body["max_login_attempts"], serde_json::json!(8));
    assert_eq!(body["max_ip_attempts"], serde_json::json!(40));
    assert_eq!(body["login_lockout_seconds"], serde_json::json!(600));
    assert_eq!(body["session_timeout_secs"], serde_json::json!(86400));
}

#[tokio::test]
async fn patch_settings_security_unauthed_returns_401() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            "",
            serde_json::json!({
                "Security": {
                    "max_login_attempts": 8,
                    "max_ip_attempts": 40,
                    "login_lockout_seconds": 600,
                    "session_timeout_secs": 86400
                }
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn patch_settings_maintenance_invalid_threshold_returns_4xx() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            &cookie,
            serde_json::json!({
                "Maintenance": {
                    "trash_retention_days": 14,
                    "audit_retention_days": 180,
                    "audit_security_retention_days": 90,
                    "disk_warn_threshold": 1.5,
                    "thumbnail_formats": "jpeg"
                }
            }),
        ))
        .await
        .unwrap();
    assert!(
        res.status().is_client_error(),
        "out-of-range disk_warn_threshold should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn patch_settings_security_invalid_attempts_returns_4xx() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            &cookie,
            serde_json::json!({
                "Security": {
                    "max_login_attempts": 0,
                    "max_ip_attempts": 40,
                    "login_lockout_seconds": 600,
                    "session_timeout_secs": 86400
                }
            }),
        ))
        .await
        .unwrap();
    assert!(
        res.status().is_client_error(),
        "max_login_attempts=0 should return 4xx, got {}",
        res.status()
    );
}

#[tokio::test]
async fn patch_settings_performance_updates() {
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
                "Performance": {
                    "max_concurrent_jobs": 16,
                    "db_maintenance_interval_hours": 12,
                    "db_vacuum_interval_hours": 72,
                    "audit_prune_interval_hours": 240,
                    "trash_purge_interval_hours": 96
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
    let body = body_json(get_res).await;
    assert_eq!(body["max_concurrent_jobs"], serde_json::json!(16));
    assert_eq!(body["db_maintenance_interval_hours"], serde_json::json!(12));
    assert_eq!(body["db_vacuum_interval_hours"], serde_json::json!(72));
    assert_eq!(body["audit_prune_interval_hours"], serde_json::json!(240));
    assert_eq!(body["trash_purge_interval_hours"], serde_json::json!(96));
}

#[tokio::test]
async fn patch_settings_performance_invalid_returns_4xx() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_patch(
            "/rest/settings",
            &cookie,
            serde_json::json!({
                "Performance": {
                    "max_concurrent_jobs": 0,
                    "db_maintenance_interval_hours": 24,
                    "db_vacuum_interval_hours": 168,
                    "audit_prune_interval_hours": 168,
                    "trash_purge_interval_hours": 168
                }
            }),
        ))
        .await
        .unwrap();
    assert!(
        res.status().is_client_error(),
        "max_concurrent_jobs=0 should return 4xx, got {}",
        res.status()
    );
}
