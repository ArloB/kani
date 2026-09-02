#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{authed_get, authed_post, body_json};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn get_library_returns_empty_list_for_fresh_db() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_get("/rest/library?page=1&page_size=20", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["items"], serde_json::json!([]));
}

#[tokio::test]
async fn scan_all_library_returns_200_for_authed_user() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_post(
            "/rest/library/scan-all",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let job_id = body["job_id"]
        .as_str()
        .expect("scan-all must return job_id");
    assert!(Uuid::parse_str(job_id).is_ok(), "job_id must be a UUID");
}

#[tokio::test]
async fn get_library_invalid_page_returns_400() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_get("/rest/library?page=0&page_size=20", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn scan_manga_all_returns_200_for_authed_user() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": "all" }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let job_id = body["job_id"]
        .as_str()
        .expect("scan with ids=all must return job_id");
    assert!(Uuid::parse_str(job_id).is_ok(), "job_id must be a UUID");
}

#[tokio::test]
async fn scan_manga_ids_empty_returns_200_for_authed_user() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": [] }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn scan_manga_invalid_body_returns_422() {
    let (app, cookie) = common::admin_app().await;

    let res = app
        .oneshot(authed_post(
            "/rest/manga/scan",
            &cookie,
            serde_json::json!({ "ids": 42 }),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "expected 4xx, got {}",
        res.status(),
    );
}
