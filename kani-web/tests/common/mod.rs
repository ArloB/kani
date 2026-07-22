#![allow(clippy::unwrap_used, dead_code)]

use axum::{Router, body::Body, http::Request};
use axum_login::{
    AuthManagerLayerBuilder,
    tower_sessions::{SessionManagerLayer, cookie::SameSite},
};
use dashmap::DashMap;
use http_body_util::BodyExt;
use kani_app::AppService;
use kani_web::{
    auth::AuthBackend, logging::RingBufferLayer, rate_limit::AuthRateLimiter, state::AppState,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64},
};
use tower::ServiceExt;
use tower_sessions_sqlx_store::SqliteStore;

pub async fn test_db() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

pub async fn test_state() -> AppState {
    let pool = test_db().await;
    let service = Arc::new(AppService::new_for_test(pool.clone()).await);
    let (_, log_handle) = RingBufferLayer::new(100);
    AppState {
        rate_limiter: Arc::new(AuthRateLimiter::new(pool, service.settings.clone())),
        csrf_secret: Arc::new([0u8; 32]),
        public_instance: false,
        service,
        proxy_secret: Arc::new([0u8; 32]),
        proxy_semaphores: moka::future::Cache::builder().max_capacity(100).build(),
        proxy_throttle: moka::future::Cache::builder().max_capacity(100).build(),
        proxy_coalesce: moka::future::Cache::builder().max_capacity(100).build(),
        proxy_bandwidth: Arc::new(DashMap::<String, Arc<AtomicU64>>::new()),
        boot_id: "test".to_string(),
        restart_requested: Arc::new(AtomicBool::new(false)),
        log_handle,
        idempotency: kani_web::idempotency::IdempotencyStore::new(),
    }
}

/// Build a testable axum router. All REST routes are mounted under `/rest`,
/// matching the production path prefix expected by `auth_guard`.
pub async fn build_test_app(state: AppState) -> Router {
    kani_web::app::build_app(state).await
}

/// Create an admin user in the given state's DB and return (username, password).
pub async fn create_admin(state: &AppState) -> (&'static str, &'static str) {
    let backend = AuthBackend::new(state.db.clone());
    let user = backend
        .create_user("admin", "admin@test.local", "Password1234!")
        .await
        .unwrap();
    backend.grant_role(user.id, "admin", None).await.unwrap();
    ("admin", "Password1234!")
}

/// Create a standard (non-admin) user and return (username, password).
pub async fn create_regular_user(
    state: &AppState,
    username: &'static str,
) -> (&'static str, &'static str) {
    let backend = AuthBackend::new(state.db.clone());
    backend
        .create_user(
            username,
            &format!("{}@test.local", username),
            "Password1234!",
        )
        .await
        .unwrap();
    (username, "Password1234!")
}

/// POST /rest/auth/login and return the session cookie string.
pub async fn login(app: &Router, username: &str, password: &str) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/rest/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({"username": username, "password": password}))
                .unwrap(),
        ))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        axum::http::StatusCode::OK,
        "login must succeed for user '{username}'"
    );

    // Keep only the name=value part (drop Secure;HttpOnly;Path=/ etc.)
    res.headers()
        .get("set-cookie")
        .expect("set-cookie header missing after login")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

pub fn get_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

pub fn delete_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

pub fn authed_get(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

pub fn authed_post(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

pub fn authed_put(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

pub fn authed_patch(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

pub fn authed_delete(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("Cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

pub fn post_json(uri: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

pub fn put_json(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Cookie", cookie)
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

/// Drain the response body as raw bytes.
#[allow(dead_code)]
pub async fn body_bytes(res: axum::response::Response) -> axum::body::Bytes {
    res.into_body().collect().await.unwrap().to_bytes()
}

/// Drain the response body and parse as JSON.
pub async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

/// Drain the response body and parse as a JSON array.
pub async fn body_array(res: axum::response::Response) -> Vec<serde_json::Value> {
    body_json(res).await.as_array().cloned().unwrap_or_default()
}

/// Build a testable axum router with both REST and OPDS routes mounted.
/// OPDS handles its own auth per-handler; the auth_guard exempts /opds paths.
pub async fn build_test_app_with_opds(state: AppState) -> Router {
    let session_store = SqliteStore::new(state.db.clone());
    session_store.migrate().await.unwrap();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);
    let auth_backend = AuthBackend::new(state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    Router::new()
        .nest("/rest", kani_web::rest::routes(state.clone()))
        .nest("/opds", kani_web::opds::routes(state.clone()))
        .layer(axum::middleware::from_fn(kani_web::auth::auth_guard))
        .layer(axum::middleware::from_fn_with_state(
            state,
            kani_web::session_touch::session_touch_middleware,
        ))
        .layer(auth_layer)
}

// DB row inserters are shared via kani-shared-test (identical across crates).
#[allow(unused_imports)]
pub use kani_shared_test::{insert_chapter, insert_manga, insert_source, insert_user};

/// Build a Basic-auth `Authorization` header value for the given credentials.
pub fn basic_auth(username: &str, password: &str) -> String {
    use base64::Engine as _;
    let encoded =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {encoded}")
}
