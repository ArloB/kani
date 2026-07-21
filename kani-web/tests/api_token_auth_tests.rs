#![allow(clippy::unwrap_used)]
// Bearer-token authentication on /rest/*: kind separation, scope enforcement,
// and the use-time intersection.

mod common;
use axum::http::StatusCode;
use common::{build_test_app, create_admin, get_req, test_state};
use kani_app::service::api_tokens::TokenKind;
use tower::ServiceExt;

fn bearer_get(path: &str, token: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .uri(path)
        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap()
}

async fn admin_user_id(state: &kani_web::state::AppState) -> kani_app::ids::UserId {
    let id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(&state.db)
        .await
        .unwrap();
    kani_app::ids::UserId(id)
}

#[tokio::test]
async fn a_scoped_api_token_reaches_a_permitted_route() {
    let state = test_state().await;
    create_admin(&state).await;
    let uid = admin_user_id(&state).await;
    let scope: kani_app::permissions::Permission = "source:browse".parse().unwrap();
    let created = state
        .service
        .create_token(uid, "bot", None, TokenKind::Api, Some(&[scope]))
        .await
        .unwrap();
    let app = build_test_app(state).await;

    let res = app
        .oneshot(bearer_get("/rest/sources", &created.raw_token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "in-scope route should succeed"
    );
}

#[tokio::test]
async fn a_token_is_forbidden_on_a_route_outside_its_scopes() {
    let state = test_state().await;
    create_admin(&state).await;
    let uid = admin_user_id(&state).await;
    let scope: kani_app::permissions::Permission = "source:browse".parse().unwrap();
    let created = state
        .service
        .create_token(uid, "read only", None, TokenKind::Api, Some(&[scope]))
        .await
        .unwrap();
    let app = build_test_app(state).await;

    let res = app
        .oneshot(bearer_get("/rest/admin/diagnostics", &created.raw_token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "a source:browse token must not reach a server:manage route"
    );
}

#[tokio::test]
async fn an_opds_token_is_rejected_on_the_rest_api() {
    let state = test_state().await;
    create_admin(&state).await;
    let uid = admin_user_id(&state).await;
    let created = state
        .service
        .create_api_token(uid, "kindle", None)
        .await
        .unwrap();
    let app = build_test_app(state).await;

    let res = app
        .oneshot(bearer_get("/rest/sources", &created.raw_token))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "acceptance keys on kind: a reader token must never reach /rest/*"
    );
}

#[tokio::test]
async fn an_invalid_bearer_is_refused_rather_than_falling_back_to_session() {
    let state = test_state().await;
    create_admin(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(bearer_get("/rest/sources", "kani_pat_totally_made_up"))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_revoked_token_stops_working() {
    let state = test_state().await;
    create_admin(&state).await;
    let uid = admin_user_id(&state).await;
    let scope: kani_app::permissions::Permission = "source:browse".parse().unwrap();
    let created = state
        .service
        .create_token(uid, "doomed", None, TokenKind::Api, Some(&[scope]))
        .await
        .unwrap();
    state
        .service
        .revoke_api_token(uid, &created.token.id)
        .await
        .unwrap();
    let app = build_test_app(state).await;

    let res = app
        .oneshot(bearer_get("/rest/sources", &created.raw_token))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn session_routes_still_work_without_any_bearer() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;

    let res = app
        .oneshot(common::authed_get("/rest/sources", &cookie))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "session auth must be unaffected"
    );

    let anon = build_test_app(test_state().await).await;
    let res = anon.oneshot(get_req("/rest/sources")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ── creation: kind requires its own permission ───────────────────────────────

fn post_json_authed(
    path: &str,
    cookie: &str,
    body: serde_json::Value,
) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(path)
        .header(axum::http::header::COOKIE, cookie)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn minting_an_api_token_requires_token_create_api() {
    let state = test_state().await;
    let (u, p) = common::create_regular_user(&state, "plain").await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;

    // A plain user can still pair a reader app...
    let opds = app
        .clone()
        .oneshot(post_json_authed(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "kindle" }),
        ))
        .await
        .unwrap();
    assert_eq!(
        opds.status(),
        StatusCode::CREATED,
        "token:create_opds is seeded wherever library:view is"
    );

    // ...but not mint something that can drive the REST API.
    let api = app
        .oneshot(post_json_authed(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "bot", "kind": "api", "scopes": ["source:browse"] }),
        ))
        .await
        .unwrap();
    assert_eq!(
        api.status(),
        StatusCode::FORBIDDEN,
        "token:create_api is not seeded broadly"
    );
}

#[tokio::test]
async fn an_admin_can_mint_a_scoped_api_token_and_sees_its_scopes() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;

    let res = app
        .clone()
        .oneshot(post_json_authed(
            "/rest/me/api-tokens",
            &cookie,
            serde_json::json!({ "name": "bot", "kind": "api", "scopes": ["source:browse"] }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let listed = app
        .oneshot(common::authed_get("/rest/me/api-tokens", &cookie))
        .await
        .unwrap();
    let body = common::body_json(listed).await;
    let row = &body.as_array().unwrap()[0];
    assert_eq!(row["kind"], "api");
    assert!(
        row["scopes"].as_str().unwrap().contains("source:browse"),
        "the UI needs the scopes to show them"
    );
    assert!(
        row["stale_scopes"].as_array().unwrap().is_empty(),
        "nothing is stale while the owner still holds the permission"
    );
}

#[tokio::test]
async fn a_scope_the_owner_lost_is_reported_as_stale() {
    let state = test_state().await;
    let (u, p) = create_admin(&state).await;
    let uid = admin_user_id(&state).await;
    let scope: kani_app::permissions::Permission = "user:manage".parse().unwrap();
    state
        .service
        .create_token(uid, "bot", None, TokenKind::Api, Some(&[scope]))
        .await
        .unwrap();

    // Demote the owner after the token was minted.
    sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_slug = 'admin'")
        .bind(uid.0)
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query("INSERT OR IGNORE INTO user_roles (user_id, role_slug) VALUES (?, 'user')")
        .bind(uid.0)
        .execute(&state.db)
        .await
        .unwrap();

    let app = build_test_app(state).await;
    let cookie = common::login(&app, u, p).await;
    let listed = app
        .oneshot(common::authed_get("/rest/me/api-tokens", &cookie))
        .await
        .unwrap();
    let body = common::body_json(listed).await;
    let stale = body.as_array().unwrap()[0]["stale_scopes"]
        .as_array()
        .unwrap();

    assert!(
        stale.iter().any(|s| s == "user:manage"),
        "a silently-dropped scope must be visible, or the user cannot tell why \
         their integration started returning 403; got {stale:?}"
    );
}
