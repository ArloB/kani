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
