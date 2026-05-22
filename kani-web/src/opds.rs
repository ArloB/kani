//! OPDS catalog handlers — mounted at /opds in main.rs.

use axum::{
    Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use serde::Deserialize;

use crate::{
    auth::{AuthBackend, AuthSession, Credentials, User},
    error::AppError,
    state::AppState,
};
use axum_login::AuthnBackend;

const ATOM_XML: &str = "application/atom+xml;profile=opds-catalog; charset=utf-8";

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/", get(opds_root))
        .route("/catalogue", get(opds_catalogue))
        .route("/manga/{id}", get(opds_manga))
        .route("/search", get(opds_search))
        .route("/opensearch", get(opds_opensearch))
        .with_state(state)
}

#[derive(Deserialize)]
struct CatalogueQuery {
    #[serde(default = "default_page")]
    page: i32,
    #[serde(default = "default_page_size")]
    page_size: i32,
    q: Option<String>,
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default = "default_page")]
    page: i32,
    q: Option<String>,
}

fn default_page() -> i32 {
    1
}
fn default_page_size() -> i32 {
    20
}

async fn opds_root(
    auth: AuthSession,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(_user) = opds_authenticate(&auth, &headers, &state).await else {
        return opds_401();
    };
    let base_url = base_url(&headers);
    let body = state.service.opds_root_feed(&base_url);
    atom_response(body)
}

async fn opds_catalogue(
    auth: AuthSession,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<CatalogueQuery>,
) -> impl IntoResponse {
    let Some(_user) = opds_authenticate(&auth, &headers, &state).await else {
        return opds_401();
    };
    let base_url = base_url(&headers);
    match state
        .service
        .opds_catalogue_feed(q.page, q.page_size, q.q, &base_url)
        .await
    {
        Ok(body) => atom_response(body),
        Err(e) => error_response(e),
    }
}

async fn opds_manga(
    auth: AuthSession,
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> impl IntoResponse {
    let Some(_user) = opds_authenticate(&auth, &headers, &state).await else {
        return opds_401();
    };
    let base_url = base_url(&headers);
    match state.service.opds_manga_feed(manga_id, &base_url).await {
        Ok(body) => atom_response(body),
        Err(e) => error_response(e),
    }
}

async fn opds_search(
    auth: AuthSession,
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> impl IntoResponse {
    let Some(_user) = opds_authenticate(&auth, &headers, &state).await else {
        return opds_401();
    };
    let base_url = base_url(&headers);
    let query = q.q.unwrap_or_default();
    match state
        .service
        .opds_search_feed(&query, q.page, &base_url)
        .await
    {
        Ok(body) => atom_response(body),
        Err(e) => error_response(e),
    }
}

async fn opds_opensearch(
    auth: AuthSession,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let Some(_user) = opds_authenticate(&auth, &headers, &state).await else {
        return opds_401();
    };
    let base_url = base_url(&headers);
    let body = state.service.opds_opensearch_description(&base_url);
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

// ─── Auth helper ─────────────────────────────────────────────────────────────

/// Tries session-cookie auth first, then HTTP Basic auth.
/// Returns `None` (caller should emit 401) if neither succeeds.
async fn opds_authenticate(
    auth: &AuthSession,
    headers: &HeaderMap,
    state: &AppState,
) -> Option<User> {
    // 1. Valid session
    if let Some(user) = &auth.user
        && user.is_active
    {
        return Some(user.clone());
    }

    // 2. Basic auth
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = BASE64_STANDARD.decode(encoded).ok()?;
    let decoded_str = std::str::from_utf8(&decoded).ok()?;
    let (username, password) = decoded_str.split_once(':')?;

    let backend = AuthBackend::new(state.service.db.clone());
    let creds = Credentials {
        username: username.to_owned(),
        password: password.to_owned(),
    };
    let user = backend.authenticate(creds).await.ok()??;
    if user.is_active { Some(user) } else { None }
}

// ─── Response helpers ─────────────────────────────────────────────────────────

fn atom_response(body: String) -> axum::response::Response {
    (StatusCode::OK, [(header::CONTENT_TYPE, ATOM_XML)], body).into_response()
}

fn opds_401() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"Kani\"")],
        "",
    )
        .into_response()
}

fn error_response(e: kani_app::error::ServiceError) -> axum::response::Response {
    let app_err: AppError = e.into();
    app_err.into_response()
}

/// Best-effort base URL derived from request headers.
fn base_url(headers: &HeaderMap) -> String {
    // X-Forwarded-Proto / X-Forwarded-Host set by reverse proxies
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost:8242");
    format!("{proto}://{host}")
}
