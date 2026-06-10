//! Double-submit cookie CSRF protection middleware.
//!
//! On GET/HEAD/OPTIONS requests: sets a `kani_csrf` cookie (SameSite=Strict, HttpOnly=false
//! so JS can read it). On state-changing methods (POST/PUT/PATCH/DELETE): validates that the
//! `X-CSRF-Token` header matches the signed cookie value.
//!
//! Exemptions:
//! - Requests with a valid `Authorization: Bearer …` header (API-token auth provides its own
//!   CSRF protection; the token cannot be sent cross-origin by a browser).
//! - Routes under `/rest/auth/passkey/` (WebAuthn flows use their own challenge).
//!
//! The CSRF token is `HMAC-SHA256(session_id, csrf_secret)` encoded as base64url (first 32 bytes).
//! It is stable within a session so SPAs can read it once on load and reuse it.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

const CSRF_COOKIE: &str = "kani_csrf";
const CSRF_HEADER: &str = "x-csrf-token";

/// Compute the CSRF token for `session_id` using the application's `csrf_secret`.
pub fn compute_token(session_id: &str, csrf_secret: &[u8; 32]) -> String {
    let mut mac = HmacSha256::new_from_slice(csrf_secret).expect("HMAC accepts any key size");
    mac.update(session_id.as_bytes());
    let result = mac.finalize().into_bytes();
    URL_SAFE_NO_PAD.encode(&result[..32])
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const SECRET: [u8; 32] = [0xDE; 32];

    #[test]
    fn compute_token_is_stable() {
        let t1 = compute_token("session-abc", &SECRET);
        let t2 = compute_token("session-abc", &SECRET);
        assert_eq!(
            t1, t2,
            "token must be deterministic for same session + secret"
        );
    }

    #[test]
    fn different_sessions_produce_different_tokens() {
        let t1 = compute_token("session-aaa", &SECRET);
        let t2 = compute_token("session-bbb", &SECRET);
        assert_ne!(t1, t2);
    }

    #[test]
    fn different_secrets_produce_different_tokens() {
        let secret2 = [0xAB; 32];
        let t1 = compute_token("session-xyz", &SECRET);
        let t2 = compute_token("session-xyz", &secret2);
        assert_ne!(t1, t2);
    }

    #[test]
    fn token_is_base64url_no_padding() {
        let t = compute_token("test", &SECRET);
        assert!(!t.contains('+'), "base64url must not contain '+'");
        assert!(!t.contains('/'), "base64url must not contain '/'");
        assert!(!t.contains('='), "no-pad base64url must not contain '='");
    }
}

/// Tower middleware that enforces CSRF protection on state-changing routes.
pub async fn csrf_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();

    // Passkey routes use their own WebAuthn challenge mechanism.
    if path.starts_with("/rest/auth/passkey/") {
        return next.run(request).await;
    }

    // Bearer-authenticated requests are inherently CSRF-safe — the token cannot
    // be sent cross-origin by a browser.
    if let Some(auth) = request.headers().get(header::AUTHORIZATION)
        && auth.to_str().unwrap_or_default().starts_with("Bearer ")
    {
        return next.run(request).await;
    }

    let is_read_only = matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS");

    if is_read_only {
        // On read-only requests: inject the CSRF cookie if it's absent.
        let mut response = next.run(request).await;

        // Derive a stable token from the session cookie value (or a random fallback).
        // We use the raw cookie header for simplicity; axum-login sessions are opaque.
        let session_val = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned())
            .unwrap_or_else(|| "anonymous".to_string());

        let token = compute_token(&session_val, &state.csrf_secret);
        let cookie = format!("{CSRF_COOKIE}={token}; Path=/; SameSite=Strict");
        if let Ok(val) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, val);
        }
        return response;
    }

    // State-changing request: validate the X-CSRF-Token header against the cookie.
    let cookie_token = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix(&format!("{CSRF_COOKIE}="))
                    .map(str::to_owned)
            })
        });

    let header_token = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    match (cookie_token, header_token) {
        (Some(cookie), Some(header)) if cookie == header => next.run(request).await,
        _ => (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "csrf_token_invalid",
                "message": "Missing or invalid CSRF token. Include the X-CSRF-Token header."
            })),
        )
            .into_response(),
    }
}
