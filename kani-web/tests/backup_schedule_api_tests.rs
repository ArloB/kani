#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_json, build_test_app, create_admin, create_regular_user,
    put_json, test_state,
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn admin_get_backup_schedule_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/backup/schedule", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("enabled").is_some());
    assert!(body.get("frequency").is_some());
    assert!(body.get("retain_n").is_some());
    assert!(body.get("destination").is_some());
}

#[tokio::test]
async fn admin_get_backup_schedule_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "alice").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/admin/backup/schedule", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_put_backup_schedule_round_trips() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let config = json!({
        "enabled": true,
        "frequency": { "type": "weekly", "weekday": 6, "hour": 4 },
        "retain_n": 3,
        "destination": { "type": "local", "path": "/tmp/kani-backups" },
        "passphrase": null
    });

    app.clone()
        .oneshot(put_json("/rest/admin/backup/schedule", &cookie, config))
        .await
        .unwrap();

    let res = app
        .oneshot(authed_get("/rest/admin/backup/schedule", &cookie))
        .await
        .unwrap();
    let body = body_json(res).await;
    assert_eq!(body["enabled"], true);
    assert_eq!(body["retain_n"], 3);
    assert_eq!(body["frequency"]["type"], "weekly");
    assert_eq!(body["frequency"]["weekday"], 6);
}

#[tokio::test]
async fn admin_backup_run_now_returns_200_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/backup/run-now",
            &cookie,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("job_id").is_some());
}

#[tokio::test]
async fn admin_backup_run_now_returns_403_for_regular_user() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/admin/backup/run-now",
            &cookie,
            json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
