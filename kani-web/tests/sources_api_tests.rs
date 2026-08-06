#![allow(clippy::unwrap_used)]
// Tests for /rest/sources endpoints: list, get, add (admin-only), delete.
// WASM upload is tested separately (it requires multipart + binary fixture).

mod common;
use axum::http::StatusCode;
use common::{
    authed_get, authed_post, body_array, body_json, build_test_app, create_admin,
    create_regular_user, get_req, login, post_json, put_json, test_state,
};
use tower::ServiceExt;

/// Creates a source as admin and returns its id.
async fn create_source(app: &axum::Router, cookie: &str, name: &str) -> i64 {
    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/sources",
            cookie,
            serde_json::json!({ "name": name }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    body_json(res).await["id"].as_i64().expect("numeric id")
}

#[tokio::test]
async fn list_sources_returns_empty_list_on_fresh_db() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/sources", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let sources = body_array(res).await;
    assert!(sources.is_empty(), "fresh DB should have no sources");
}

#[tokio::test]
async fn list_sources_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/sources")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_source_returns_404_for_missing_id() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_get("/rest/sources/999999", &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("not_found"));
}

#[tokio::test]
async fn add_source_requires_source_install_permission() {
    let state = test_state().await;
    // Regular user role does not have source:install permission.
    let (username, password) = create_regular_user(&state, "bob").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "my-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json(res).await;
    assert_eq!(body["code"], serde_json::json!("forbidden"));
}

#[tokio::test]
async fn add_source_returns_201_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .clone()
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "test-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert!(
        body["id"].is_number(),
        "response must contain numeric id, got: {body}"
    );

    let list_res = app
        .oneshot(authed_get("/rest/sources", &cookie))
        .await
        .unwrap();
    let sources = body_array(list_res).await;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["name"], serde_json::json!("test-source"));
}

#[tokio::test]
async fn add_source_returns_400_for_empty_name() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": ""}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_source_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app.oneshot(get_req("/rest/sources/1")).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn add_source_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(post_json(
            "/rest/sources",
            serde_json::json!({"name": "test-source"}),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn get_source_returns_200_for_authed_user() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let create_res = app
        .clone()
        .oneshot(authed_post(
            "/rest/sources",
            &cookie,
            serde_json::json!({"name": "fetch-me"}),
        ))
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::CREATED);
    let created = body_json(create_res).await;
    let id = created["id"].as_i64().expect("id must be numeric");

    let res = app
        .oneshot(authed_get(&format!("/rest/sources/{id}"), &cookie))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["name"], serde_json::json!("fetch-me"));
}

#[tokio::test]
async fn set_browser_enabled_returns_200_and_persists_for_admin() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let id = create_source(&app, &cookie, "browser-src").await;

    let res = app
        .clone()
        .oneshot(put_json(
            &format!("/rest/sources/{id}/browser-enabled"),
            &cookie,
            serde_json::json!({ "enabled": false }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get_res = app
        .oneshot(authed_get(&format!("/rest/sources/{id}"), &cookie))
        .await
        .unwrap();
    let body = body_json(get_res).await;
    assert_eq!(body["browser_enabled"], serde_json::json!(false));
}

#[tokio::test]
async fn set_browser_enabled_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let req = axum::http::Request::builder()
        .method("PUT")
        .uri("/rest/sources/1/browser-enabled")
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(r#"{"enabled":false}"#))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn set_browser_enabled_requires_source_install_permission() {
    let state = test_state().await;
    let (username, password) = create_regular_user(&state, "carol").await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            "/rest/sources/1/browser-enabled",
            &cookie,
            serde_json::json!({ "enabled": false }),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn set_browser_enabled_rejects_invalid_body() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(put_json(
            "/rest/sources/1/browser-enabled",
            &cookie,
            serde_json::json!({ "wrong_field": true }),
        ))
        .await
        .unwrap();

    assert!(
        res.status().is_client_error(),
        "missing `enabled` field should be a 4xx, got {}",
        res.status()
    );
}

// ── Bulk capabilities ─────────────────────────────────────────────────────────
//
// The point of this endpoint is one round trip instead of one request per
// source, so the tests check the shape a client depends on — and the route
// ordering, which is the way this breaks silently.

#[tokio::test]
async fn bulk_capabilities_returns_200_with_auth() {
    let state = test_state().await;
    let (user, pass) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, user, pass).await;

    create_source(&app, &cookie, "alpha").await;
    create_source(&app, &cookie, "beta").await;

    let res = app
        .clone()
        .oneshot(authed_get("/rest/sources/capabilities", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = body_array(res).await;
    assert_eq!(body.len(), 2, "one entry per installed source");
    for entry in &body {
        assert!(
            entry.get("source_id").is_some(),
            "each entry names its source"
        );
        assert_eq!(
            entry.get("streaming_chapters").and_then(|v| v.as_bool()),
            Some(true),
            "capability flags are flattened, not nested"
        );
    }
}

#[tokio::test]
async fn bulk_capabilities_returns_401_without_auth() {
    let state = test_state().await;
    let app = build_test_app(state).await;
    let res = app
        .oneshot(get_req("/rest/sources/capabilities"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bulk_capabilities_is_empty_not_an_error_with_no_sources() {
    let state = test_state().await;
    let (user, pass) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, user, pass).await;

    let res = app
        .clone()
        .oneshot(authed_get("/rest/sources/capabilities", &cookie))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(body_array(res).await.is_empty());
}

#[tokio::test]
async fn bulk_route_is_not_swallowed_by_the_per_source_route() {
    // `/sources/{id}/capabilities` is registered too. If the parameterised
    // route were matched first, "capabilities" would be parsed as an id and
    // this would 400 or 404 rather than listing.
    let state = test_state().await;
    let (user, pass) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, user, pass).await;
    let id = create_source(&app, &cookie, "gamma").await;

    let bulk = app
        .clone()
        .oneshot(authed_get("/rest/sources/capabilities", &cookie))
        .await
        .unwrap();
    assert_eq!(bulk.status(), StatusCode::OK, "bulk route must win");

    // And the per-source route still works alongside it.
    let single = app
        .clone()
        .oneshot(authed_get(
            &format!("/rest/sources/{id}/capabilities"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(single.status(), StatusCode::OK);
    let one = body_json(single).await;
    assert_eq!(
        one.get("streaming_chapters").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[tokio::test]
async fn bulk_and_per_source_agree() {
    // Two endpoints answering the same question must not drift.
    let state = test_state().await;
    let (user, pass) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, user, pass).await;
    let id = create_source(&app, &cookie, "delta").await;

    let bulk = body_array(
        app.clone()
            .oneshot(authed_get("/rest/sources/capabilities", &cookie))
            .await
            .unwrap(),
    )
    .await;
    let single = body_json(
        app.clone()
            .oneshot(authed_get(
                &format!("/rest/sources/{id}/capabilities"),
                &cookie,
            ))
            .await
            .unwrap(),
    )
    .await;

    let from_bulk = bulk
        .iter()
        .find(|e| e.get("source_id").and_then(|v| v.as_i64()) == Some(id))
        .expect("the created source appears in the bulk listing");
    assert_eq!(
        from_bulk.get("streaming_chapters"),
        single.get("streaming_chapters"),
        "bulk and per-source disagree about the same source"
    );
}
