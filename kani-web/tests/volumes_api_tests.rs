#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{authed_delete, authed_get, build_test_app, create_admin, put_json, test_state};
use serde_json::json;
use tower::ServiceExt;

/// Seeds a source, a manga, and one chapter, returning `(manga_id, chapter_id)`.
async fn seed_manga_with_chapter(state: &kani_web::state::AppState) -> (i64, i64) {
    // `sources.name` is unique, so a test seeding twice needs distinct names.
    let unique = uuid::Uuid::new_v4().to_string();

    let source_id: i64 =
        sqlx::query_scalar("INSERT INTO sources (name, version) VALUES (?, '0.1') RETURNING id")
            .bind(format!("src-{unique}"))
            .fetch_one(&state.db)
            .await
            .unwrap();

    let manga_id: i64 = sqlx::query_scalar(
        "INSERT INTO manga (source_id, source_manga_id, name, status) \
         VALUES (?, ?, 'Manga', 0) RETURNING id",
    )
    .bind(source_id)
    .bind(&unique)
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
async fn a_created_volume_is_listed_back_with_its_name() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (manga_id, _) = seed_manga_with_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let created = app
        .clone()
        .oneshot(common::authed_post(
            &format!("/rest/manga/{manga_id}/volumes"),
            &cookie,
            json!({ "name": "Volume 1", "volume_num": 1 }),
        ))
        .await
        .unwrap();
    assert!(
        created.status().is_success(),
        "creating a volume failed: {}",
        created.status()
    );

    let listed = app
        .oneshot(authed_get(
            &format!("/rest/manga/{manga_id}/volumes"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);

    let body = common::body_json(listed).await;
    let volumes = body
        .get("volumes")
        .and_then(|v| v.as_array())
        .or_else(|| body.as_array())
        .expect("the listing must be an array of volumes");
    assert_eq!(
        volumes.len(),
        1,
        "exactly the created volume must come back"
    );
    assert_eq!(volumes[0]["name"], "Volume 1");
    assert_eq!(volumes[0]["manga_id"], manga_id);
    // `time`'s default serde emits an array the browser cannot parse.
    assert!(
        volumes[0]["created_at"].is_string(),
        "created_at must serialise as an RFC 3339 string, got {}",
        volumes[0]["created_at"]
    );
}

#[tokio::test]
async fn volumes_of_one_manga_do_not_leak_into_another() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let (mine, _) = seed_manga_with_chapter(&state).await;
    let (theirs, _) = seed_manga_with_chapter(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    app.clone()
        .oneshot(common::authed_post(
            &format!("/rest/manga/{mine}/volumes"),
            &cookie,
            json!({ "name": "Only Mine", "volume_num": 1 }),
        ))
        .await
        .unwrap();

    let listed = app
        .oneshot(authed_get(
            &format!("/rest/manga/{theirs}/volumes"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);

    let body = common::body_json(listed).await;
    let volumes = body
        .get("volumes")
        .and_then(|v| v.as_array())
        .or_else(|| body.as_array())
        .expect("the listing must be an array of volumes");
    assert!(
        volumes.is_empty(),
        "a volume must not appear under a different manga, got {volumes:?}"
    );
}

#[tokio::test]
async fn delete_volume_returns_404_for_missing_volume() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(authed_delete("/rest/manga/1/volumes/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assign_chapter_volume_returns_404_for_missing_chapter() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            "/rest/manga/1/chapters/999999/volume",
            &cookie,
            json!({ "volume_id": null }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
