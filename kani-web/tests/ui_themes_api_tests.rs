#![allow(clippy::unwrap_used)]

//! Plan 05 Phase 2 — `/rest/ui/themes`.
//!
//! The routes only require an authenticated user, because a user manages their
//! own themes. `theme:publish` is checked inside the handlers, so the tests that
//! matter most are the ones proving a regular user cannot reach the
//! instance-wide theme by any route: publishing it, editing it, or deleting it.

mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{
    authed_get, authed_post, build_test_app, create_admin, create_regular_user, login, test_state,
};
use http_body_util::BodyExt as _;
use tower::ServiceExt;

fn theme_body(name: &str, instance_wide: bool) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "tokens": { "--color-accent": "#b93a24" },
        "instance_wide": instance_wide,
    })
}

fn put(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

fn del(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

async fn json_of(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ── The triplet ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_themes_returns_200_for_an_authed_user() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(authed_get("/rest/ui/themes", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = json_of(res).await;
    assert!(body["themes"].is_array());
    assert!(body["active_id"].is_null(), "nothing is active yet");
}

#[tokio::test]
async fn list_themes_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/rest/ui/themes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_unknown_token_is_rejected() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let body = serde_json::json!({
        "name": "Bad",
        "tokens": { "--evil": "#000" },
    });
    let res = app
        .oneshot(authed_post("/rest/ui/themes", &cookie, body))
        .await
        .unwrap();
    assert!(
        res.status().is_client_error(),
        "an unknown token must be refused, got {}",
        res.status()
    );
}

// ── Round trip ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_theme_can_be_created_activated_and_deleted() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let created = json_of(
        app.clone()
            .oneshot(authed_post(
                "/rest/ui/themes",
                &cookie,
                theme_body("Midnight", false),
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["instance_wide"], false);

    let res = app
        .clone()
        .oneshot(put(&format!("/rest/ui/themes/{id}/activate"), &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let listed = json_of(
        app.clone()
            .oneshot(authed_get("/rest/ui/themes", &cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed["active_id"], serde_json::json!(id));

    let res = app
        .clone()
        .oneshot(put("/rest/ui/themes/deactivate", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(del(&format!("/rest/ui/themes/{id}"), &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let listed = json_of(
        app.oneshot(authed_get("/rest/ui/themes", &cookie))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed["themes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn activating_an_unknown_theme_is_404() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(put("/rest/ui/themes/nope/activate", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ── theme:publish boundary — the reason the check is in the handler ───────────

#[tokio::test]
async fn a_regular_user_cannot_publish_an_instance_wide_theme() {
    let state = test_state().await;
    create_admin(&state).await;
    let (u, p) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let res = app
        .oneshot(authed_post(
            "/rest/ui/themes",
            &cookie,
            theme_body("Everyone's", true),
        ))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "instance_wide requires theme:publish"
    );
}

#[tokio::test]
async fn an_admin_can_publish_an_instance_wide_theme_and_others_see_it() {
    let state = test_state().await;
    let (au, ap) = create_admin(&state).await;
    let (bu, bp) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;

    let admin = login(&app, au, ap).await;
    let published = json_of(
        app.clone()
            .oneshot(authed_post(
                "/rest/ui/themes",
                &admin,
                theme_body("House Style", true),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(published["instance_wide"], true);

    let bob = login(&app, bu, bp).await;
    let listed = json_of(
        app.oneshot(authed_get("/rest/ui/themes", &bob))
            .await
            .unwrap(),
    )
    .await;
    let names: Vec<&str> = listed["themes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert_eq!(names, vec!["House Style"], "bob sees the published theme");
}

#[tokio::test]
async fn a_regular_user_cannot_delete_the_instance_theme() {
    let state = test_state().await;
    let (au, ap) = create_admin(&state).await;
    let (bu, bp) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;

    let admin = login(&app, au, ap).await;
    let published = json_of(
        app.clone()
            .oneshot(authed_post(
                "/rest/ui/themes",
                &admin,
                theme_body("House Style", true),
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = published["id"].as_str().unwrap().to_string();

    let bob = login(&app, bu, bp).await;
    let res = app
        .clone()
        .oneshot(del(&format!("/rest/ui/themes/{id}"), &bob))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // And it is still there.
    let listed = json_of(
        app.oneshot(authed_get("/rest/ui/themes", &admin))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(listed["themes"].as_array().unwrap().len(), 1);
}

// Editing is authorised by who *owns* the row, not by the flag in the body —
// otherwise omitting `instance_wide` would be enough to rewrite the published
// theme for everyone.
#[tokio::test]
async fn a_regular_user_cannot_edit_the_instance_theme_by_omitting_the_flag() {
    let state = test_state().await;
    let (au, ap) = create_admin(&state).await;
    let (bu, bp) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;

    let admin = login(&app, au, ap).await;
    let published = json_of(
        app.clone()
            .oneshot(authed_post(
                "/rest/ui/themes",
                &admin,
                theme_body("House Style", true),
            ))
            .await
            .unwrap(),
    )
    .await;
    let id = published["id"].as_str().unwrap().to_string();

    let bob = login(&app, bu, bp).await;
    let mut hijack = theme_body("Hijacked", false);
    hijack["id"] = serde_json::json!(id);
    let res = app
        .clone()
        .oneshot(authed_post("/rest/ui/themes", &bob, hijack))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let listed = json_of(
        app.oneshot(authed_get("/rest/ui/themes", &admin))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        listed["themes"][0]["name"], "House Style",
        "the published theme is unchanged"
    );
}

// The stored CSS is the sanitised output, so what a later GET returns is
// already safe even though the client posted something that was not.
#[tokio::test]
async fn posted_custom_css_comes_back_sanitised() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, u, p).await;

    let mut body = theme_body("Styled", false);
    body["custom_css"] = serde_json::json!("@import url(evil.css); .btn { color: red }");
    app.clone()
        .oneshot(authed_post("/rest/ui/themes", &cookie, body))
        .await
        .unwrap();

    let listed = json_of(
        app.oneshot(authed_get("/rest/ui/themes", &cookie))
            .await
            .unwrap(),
    )
    .await;
    let css = listed["themes"][0]["custom_css"].as_str().unwrap();
    assert!(
        !css.contains("@import") && !css.contains("url("),
        "GET must not hand back what was posted: {css}"
    );
    assert!(css.contains("color: red"), "and the safe part survives");
}
