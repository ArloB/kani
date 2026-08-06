#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use axum::{body::Body, http::Request};
use common::{authed_get, body_json, build_test_app, create_admin, login, put_json, test_state};
use tower::ServiceExt;

fn authed_post_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

async fn insert_test_chapter(state: &kani_web::state::AppState) -> (i64, i64) {
    let src_id: i64 = sqlx::query_scalar(
        "INSERT INTO sources (name, version) VALUES ('src', '0.1') RETURNING id",
    )
    .fetch_one(&state.db)
    .await
    .unwrap();

    let manga_id: i64 = sqlx::query_scalar(
        "INSERT INTO manga (source_id, source_manga_id, name, status) \
         VALUES (?, 'mid', 'Manga', 0) RETURNING id",
    )
    .bind(src_id)
    .fetch_one(&state.db)
    .await
    .unwrap();

    let chapter_id: i64 = sqlx::query_scalar(
        "INSERT INTO chapters (manga_id, source_chapter_id, chapter_number, language) \
         VALUES (?, 'c1', 1.0, 'en') RETURNING id",
    )
    .bind(manga_id)
    .fetch_one(&state.db)
    .await
    .unwrap();

    (manga_id, chapter_id)
}

#[tokio::test]
async fn get_bookmarks_returns_200_authed() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/chapter/{chapter_id}/bookmarks"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body, serde_json::json!([]));
}

#[tokio::test]
async fn get_bookmarks_returns_401_unauthenticated() {
    let state = test_state().await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/chapter/{chapter_id}/bookmarks"),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
#[tokio::test]
async fn toggle_bookmark_adds_and_returns_state() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_post_json(
            &format!("/rest/chapter/{chapter_id}/bookmarks"),
            &cookie,
            serde_json::json!({ "page_index": 3 }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["bookmarked"], serde_json::json!(true));
}

#[tokio::test]
async fn get_chapter_note_returns_200_authed() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/chapter/{chapter_id}/note"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["note"], serde_json::json!(null));
}

#[tokio::test]
async fn get_chapter_note_returns_401_unauthenticated() {
    let state = test_state().await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(authed_get(&format!("/rest/chapter/{chapter_id}/note"), ""))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_chapter_note_returns_204_authed() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            &format!("/rest/chapter/{chapter_id}/note"),
            &cookie,
            serde_json::json!({ "note": "interesting chapter" }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn set_chapter_note_returns_401_unauthenticated() {
    let state = test_state().await;
    let (_, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(put_json(
            &format!("/rest/chapter/{chapter_id}/note"),
            "",
            serde_json::json!({ "note": "test" }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_manga_chapter_notes_returns_notes_object() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (manga_id, chapter_id) = insert_test_chapter(&state).await;
    let app = build_test_app(state.clone()).await;
    let cookie = login(&app, username, password).await;

    app.clone()
        .oneshot(put_json(
            &format!("/rest/chapter/{chapter_id}/note"),
            &cookie,
            serde_json::json!({ "note": "chapter note text" }),
        ))
        .await
        .unwrap();

    let res = app
        .oneshot(authed_get(
            &format!("/rest/manga/{manga_id}/chapter-notes"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    let notes = body["notes"].as_array().expect("notes should be an array");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["chapter_id"], serde_json::json!(chapter_id));
    assert_eq!(notes[0]["note"], serde_json::json!("chapter note text"));
}

#[tokio::test]
async fn get_manga_chapter_notes_returns_401_unauthenticated() {
    let state = test_state().await;
    let (manga_id, _) = insert_test_chapter(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(authed_get(
            &format!("/rest/manga/{manga_id}/chapter-notes"),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
