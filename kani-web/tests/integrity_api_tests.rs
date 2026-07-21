#![allow(clippy::unwrap_used)]
// The scrub REST surface: report retrieval and the explicit orphan deletion
// that replaced `integrity-check?fix=true`.

mod common;
use axum::http::StatusCode;
use common::{authed_get, build_test_app, create_admin, create_regular_user, test_state};
use serde_json::json;
use tower::ServiceExt;

fn post(
    path: &str,
    cookie: Option<&str>,
    body: serde_json::Value,
) -> axum::http::Request<axum::body::Body> {
    let mut b = axum::http::Request::builder()
        .method("POST")
        .uri(path)
        .header(axum::http::header::CONTENT_TYPE, "application/json");
    if let Some(c) = cookie {
        b = b.header(axum::http::header::COOKIE, c);
    }
    b.body(axum::body::Body::from(body.to_string())).unwrap()
}

// ── GET /rest/admin/library/scrub/last ───────────────────────────────────────

#[tokio::test]
async fn last_scrub_is_null_before_any_run() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;

    let res = app
        .oneshot(authed_get("/rest/admin/library/scrub/last", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        common::body_json(res).await.is_null(),
        "no run yet must be null, not an empty report that reads as a clean bill"
    );
}

#[tokio::test]
async fn last_scrub_returns_the_persisted_report() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    state
        .service
        .scrub_library(kani_app::service::integrity::ScrubDepth::Quick, false, None)
        .await
        .unwrap();

    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;
    let res = app
        .oneshot(authed_get("/rest/admin/library/scrub/last", &cookie))
        .await
        .unwrap();
    let body = common::body_json(res).await;

    assert_eq!(body["depth"], "quick");
    assert!(body["created_at"].as_i64().unwrap() > 0);
    assert!(
        body["report"]["checked"].is_number(),
        "the report must survive the round trip through storage, got {body}"
    );
}

#[tokio::test]
async fn last_scrub_requires_admin() {
    let state = test_state().await;
    let (u, p) = create_regular_user(&state, "dave").await;
    let app = build_test_app(state).await;

    let anon = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/rest/admin/library/scrub/last")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

    let cookie = common::login(&app, u, p).await;
    let plain = app
        .oneshot(authed_get("/rest/admin/library/scrub/last", &cookie))
        .await
        .unwrap();
    assert_eq!(plain.status(), StatusCode::FORBIDDEN);
}

// ── POST /rest/admin/library/orphans/delete ──────────────────────────────────

#[tokio::test]
async fn orphan_delete_requires_admin() {
    let state = test_state().await;
    let (u, p) = create_regular_user(&state, "erin").await;
    let app = build_test_app(state).await;

    let anon = app
        .clone()
        .oneshot(post(
            "/rest/admin/library/orphans/delete",
            None,
            json!({ "paths": [] }),
        ))
        .await
        .unwrap();
    assert_eq!(anon.status(), StatusCode::UNAUTHORIZED);

    let cookie = common::login(&app, u, p).await;
    let plain = app
        .oneshot(post(
            "/rest/admin/library/orphans/delete",
            Some(&cookie),
            json!({ "paths": [] }),
        ))
        .await
        .unwrap();
    assert_eq!(plain.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn orphan_delete_defaults_to_a_dry_run() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let library = { state.service.settings.read().await.library_path.clone() };
    let victim = library.join("Some Manga - 1").join("orphan.cbz");
    std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
    std::fs::write(&victim, b"not really a cbz").unwrap();

    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;
    let res = app
        .oneshot(post(
            "/rest/admin/library/orphans/delete",
            Some(&cookie),
            json!({ "paths": [victim.to_string_lossy()] }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = common::body_json(res).await;
    assert_eq!(
        body["dry_run"], true,
        "omitting dry_run must preview, never delete — the destructive reading \
         of a missing field is the one that loses data"
    );
    assert!(victim.exists());
}

#[tokio::test]
async fn orphan_delete_removes_only_what_it_is_given() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let library = { state.service.settings.read().await.library_path.clone() };
    let dir = library.join("Some Manga - 1");
    std::fs::create_dir_all(&dir).unwrap();
    let doomed = dir.join("doomed.cbz");
    let spared = dir.join("spared.cbz");
    std::fs::write(&doomed, b"x").unwrap();
    std::fs::write(&spared, b"y").unwrap();

    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;
    let res = app
        .oneshot(post(
            "/rest/admin/library/orphans/delete",
            Some(&cookie),
            json!({ "paths": [doomed.to_string_lossy()], "dry_run": false }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(common::body_json(res).await["removed_count"], 1);
    assert!(!doomed.exists());
    assert!(
        spared.exists(),
        "deletion must be scoped to the listed paths"
    );
}
