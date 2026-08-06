#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{authed_get, body_json, build_test_app, create_admin, login, put_json, test_state};
use kani_web::state::AppState;
use tower::ServiceExt;

/// Insert a minimal source + manga row into the test DB; returns the manga id.
async fn insert_test_manga(state: &AppState) -> i64 {
    let src_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, version) VALUES ('src', '0.1') RETURNING id",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();

    sqlx::query_scalar(
        "INSERT INTO manga (source_id, source_manga_id, name, status) \
         VALUES (?, 'mid1', 'Test Manga', 0) RETURNING id",
    )
    .bind(src_id)
    .fetch_one(&state.db)
    .await
    .unwrap()
}

#[tokio::test]
async fn put_reader_prefs_returns_204() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let manga_id = insert_test_manga(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            &format!("/rest/manga/{manga_id}/tracking"),
            &cookie,
            serde_json::json!({ "reader_prefs": r#"{"mode":"webtoon"}"# }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn put_reader_prefs_is_returned_by_get_tracking() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let manga_id = insert_test_manga(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    app.clone()
        .oneshot(put_json(
            &format!("/rest/manga/{manga_id}/tracking"),
            &cookie,
            serde_json::json!({ "reader_prefs": r#"{"mode":"scroll","fit":"width"}"# }),
        ))
        .await
        .unwrap();

    let res = app
        .oneshot(authed_get(
            &format!("/rest/manga/{manga_id}/tracking"),
            &cookie,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(
        body["reader_prefs"],
        serde_json::json!(r#"{"mode":"scroll","fit":"width"}"#)
    );
}

#[tokio::test]
async fn put_reader_prefs_returns_401_without_auth() {
    let state = test_state().await;
    let manga_id = insert_test_manga(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(put_json(
            &format!("/rest/manga/{manga_id}/tracking"),
            "",
            serde_json::json!({ "reader_prefs": r#"{"mode":"webtoon"}"# }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_reader_prefs_rejects_non_object_json() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let manga_id = insert_test_manga(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            &format!("/rest/manga/{manga_id}/tracking"),
            &cookie,
            serde_json::json!({ "reader_prefs": "[1,2,3]" }),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "non-object JSON must be rejected with a 4xx, got {}",
        res.status()
    );
}
