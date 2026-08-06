#![allow(clippy::unwrap_used)]

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_json, build_test_app, create_admin, get_req, login, post_json,
    test_state,
};
use tower::ServiceExt;

#[tokio::test]
async fn system_info_returns_200_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/rest/system/info")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("version").is_some());
    assert!(body.get("first_run").is_some());
    assert!(body.get("oidc_available").is_some());
    assert!(body.get("registration_enabled").is_some());
}

#[tokio::test]
async fn system_info_version_matches_cargo_pkg_version() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/rest/system/info")).await.unwrap();
    let body = body_json(res).await;
    assert_eq!(body["version"].as_str().unwrap(), kani_web::KANI_VERSION);
}

#[tokio::test]
async fn system_info_reports_first_run_true_on_fresh_db() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/rest/system/info")).await.unwrap();
    let body = body_json(res).await;
    assert!(body["first_run"].as_bool().unwrap());
}

#[tokio::test]
async fn complete_first_run_requires_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(post_json(
            "/rest/system/first-run-complete",
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn swagger_ui_returns_200() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app.oneshot(get_req("/api-docs/")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn openapi_json_contains_expected_paths() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(get_req("/api-docs/openapi.json"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let spec = body_json(res).await;
    assert!(spec["paths"].is_object(), "spec should have paths object");
    assert!(
        spec["paths"]["/rest/system/info"].is_object(),
        "system/info should be documented"
    );
}

#[tokio::test]
async fn complete_first_run_flips_flag() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/system/first-run-complete",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res2 = app.oneshot(get_req("/rest/system/info")).await.unwrap();
    let body = body_json(res2).await;
    assert!(!body["first_run"].as_bool().unwrap());
}

#[tokio::test]
async fn system_changelog_returns_401_unauthenticated() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(get_req("/rest/system/changelog"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn system_changelog_returns_rendered_html() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/system/changelog", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert!(body.get("version").is_some());
    let html = body["html"].as_str().expect("html is a string");
    assert!(
        html.contains('<'),
        "the changelog must arrive as rendered HTML, not raw markdown: {html}"
    );
    assert!(
        !html.contains("# Changelog"),
        "markdown headings must be rendered, not passed through: {html}"
    );
}

#[tokio::test]
async fn system_changelog_html_is_sanitised() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/system/changelog", &cookie))
        .await
        .unwrap();
    let body = body_json(res).await;
    let html = body["html"].as_str().unwrap();
    assert!(
        !html.contains("<script"),
        "sanitised output has no <script>"
    );
    assert!(
        !html.contains("onerror="),
        "sanitised output has no handlers"
    );
}
