//! Reusable REST router assembly with session/auth middleware.
//!
//! `build_app` is the testable core of the HTTP layer: it wires up the
//! session store, axum_login auth layer, and REST routes without adding
//! production-only concerns like rate-limiting, compression, or static files.
//! Tests call it directly; `main.rs` calls it and adds the rest.

use axum::Router;
use axum_login::{
    AuthManagerLayerBuilder,
    tower_sessions::{SessionManagerLayer, cookie::SameSite},
};
use tower_sessions_sqlx_store::SqliteStore;

use crate::{auth::AuthBackend, rest, state::AppState};

/// Build the REST API + auth/session stack as an axum [`Router`].
///
/// All REST endpoints are mounted under `/rest` so that the
/// [`crate::auth::auth_guard`] path predicates (which check for
/// `/rest/auth/` as the public prefix) function correctly.
///
/// Does **not** include rate-limiting, response compression, CORS, static
/// file serving, or the OPDS catalog — the production `main.rs` adds those
/// on top of this router.
pub async fn build_app(state: AppState) -> Router {
    let session_store = SqliteStore::new(state.db.clone());
    session_store
        .migrate()
        .await
        .expect("session store migration failed");

    let secure_cookies = std::env::var("KANI_SECURE_COOKIES")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);

    let auth_backend = AuthBackend::new(state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    Router::new()
        .nest("/rest", rest::routes(state))
        .layer(axum::middleware::from_fn(crate::auth::auth_guard))
        .layer(auth_layer)
}
