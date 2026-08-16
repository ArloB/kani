#![allow(clippy::unwrap_used)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{authed_post, build_test_app, create_admin, login, test_state};
use tower::ServiceExt;

fn post_without_csrf(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from("{}"))
        .unwrap()
}

#[tokio::test]
async fn a_session_write_without_the_csrf_header_is_refused() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(post_without_csrf("/rest/library/scan_all", &cookie))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "a forged write carrying only the session cookie must be refused"
    );
    let body = common::body_json(res).await;
    assert_eq!(body["error"], "csrf_token_invalid");
}

#[tokio::test]
async fn a_session_write_with_the_matching_token_is_allowed() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(authed_post(
            "/rest/library/scan_all",
            &cookie,
            serde_json::json!({}),
        ))
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::FORBIDDEN,
        "the double-submit pair the helpers build must satisfy the middleware"
    );
}

#[tokio::test]
async fn a_token_minted_for_another_session_is_refused() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let mine = login(&app, username, password).await;
    let theirs = login(&app, username, password).await;

    // The attacker echoes a token they legitimately hold, against a different
    // session's cookie. Binding the token to the session is what defeats this.
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rest/library/scan_all")
                .header("Content-Type", "application/json")
                .header(
                    "Cookie",
                    format!("{mine}; kani_csrf={}", common::csrf_token(&theirs)),
                )
                .header("X-CSRF-Token", common::csrf_token(&theirs))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn an_unauthenticated_write_still_answers_401_not_403() {
    let state = test_state().await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/rest/library/scan_all")
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "a sessionless request has no ambient authority to forge, so auth answers first"
    );
}

#[tokio::test]
async fn a_read_hands_back_a_usable_token() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;
    let cookie = login(&app, username, password).await;

    let res = app
        .oneshot(common::authed_get("/rest/auth/current_user", &cookie))
        .await
        .unwrap();

    let issued: Vec<String> = res
        .headers()
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter(|v| v.starts_with("kani_csrf="))
        .map(str::to_owned)
        .collect();

    assert_eq!(issued.len(), 1, "a read must issue exactly one CSRF cookie");
    let value = issued[0]
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("kani_csrf=");
    assert_eq!(
        value,
        common::csrf_token(&cookie),
        "the issued cookie must be the token the client is expected to echo"
    );
    assert!(
        issued[0].contains("SameSite=Strict"),
        "the CSRF cookie must not travel cross-site"
    );
}

#[tokio::test]
async fn logging_in_is_reachable_without_a_prior_token() {
    let state = test_state().await;
    let (username, password) = create_admin(&state).await;
    let app = build_test_app(state).await;

    let res = app
        .oneshot(common::post_json(
            "/rest/auth/login",
            serde_json::json!({ "username": username, "password": password }),
        ))
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        StatusCode::OK,
        "requiring a session-bound token to reach login would be circular"
    );
}
