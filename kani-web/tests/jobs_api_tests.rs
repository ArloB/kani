#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use kani_shared_test::{insert_manga, insert_source};
use tower::ServiceExt;

#[tokio::test]
async fn list_jobs_401_unauthenticated() {
    let state = common::test_state().await;
    let app = common::build_test_app(state).await;
    let res = app.oneshot(common::get_req("/rest/jobs")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_jobs_403_regular_user() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_regular_user(&state, "bob").await;
    let cookie = common::login(&app, "bob", "Password1234!").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_get("/rest/jobs", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn list_jobs_200_admin() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_get("/rest/jobs", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = common::body_json(res).await;
    assert!(
        body["jobs"].is_array(),
        "jobs list is paged: {{ jobs, total }}"
    );
    assert_eq!(body["total"], serde_json::json!(0));
}

#[tokio::test]
async fn list_jobs_accepts_comma_separated_status_filter() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_get(
            "/rest/jobs?status=pending,running&limit=10",
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = common::body_json(res).await;
    assert!(body["jobs"].is_array());
}

#[tokio::test]
async fn get_job_404_nonexistent() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let fake_id = uuid::Uuid::new_v4();
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_get(
            &format!("/rest/jobs/{fake_id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_job_401_unauthenticated() {
    let state = common::test_state().await;
    let app = common::build_test_app(state).await;
    let fake_id = uuid::Uuid::new_v4();
    let res = app
        .oneshot(common::delete_req(&format!("/rest/jobs/{fake_id}")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cancel_job_404_nonexistent() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let fake_id = uuid::Uuid::new_v4();
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_delete(
            &format!("/rest/jobs/{fake_id}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn download_all_401_unauthenticated() {
    let state = common::test_state().await;
    let source_id = insert_source(&state.db, "src").await;
    let manga_id = insert_manga(&state.db, source_id, "m1", "Manga").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::post_json(
            &format!("/rest/manga/{}/download_all", manga_id),
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn download_all_404_nonexistent_manga() {
    let state = common::test_state().await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_post(
            "/rest/manga/99999/download_all",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn download_all_200_returns_job_id() {
    let state = common::test_state().await;
    let source_id = insert_source(&state.db, "src").await;
    let manga_id = insert_manga(&state.db, source_id, "m1", "Manga").await;
    let app = common::build_test_app(state.clone()).await;
    common::create_admin(&state).await;
    let cookie = common::login(&app, "admin", "Password1234!").await;
    let app = common::build_test_app(state).await;
    let res = app
        .oneshot(common::authed_post(
            &format!("/rest/manga/{}/download_all", manga_id),
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = common::body_json(res).await;
    assert!(
        body.get("job_id").and_then(|v| v.as_str()).is_some(),
        "response must contain a job_id string field; got: {body}"
    );
}
