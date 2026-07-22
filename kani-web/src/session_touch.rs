//! Tower middleware that enforces per-session revocation and maintains the
//! `user_sessions` sidecar table.
//!
//! Revocation used to be written and never read: `revoke_session` set
//! `revoked_at`, the row disappeared from the session-inventory UI, and the
//! cookie went on working indefinitely. The one function written to close the
//! loop had no callers. This is where the loop closes — it is the only
//! middleware that already holds both the resolved user and the tower-sessions
//! id.

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_login::AuthSession;

use crate::{auth::AuthBackend, state::AppState};

/// Middleware: if the request is authenticated, record a session touch.
/// Passes through unconditionally — errors are non-fatal.
pub async fn session_touch_middleware(
    State(state): State<AppState>,
    mut auth: AuthSession<AuthBackend>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(user) = &auth.user {
        // Extract the tower-sessions session ID from the cookie header so we can
        // correlate with the user_sessions table.
        let session_id = auth.session.id().map(|id| id.to_string());
        if let Some(sid) = session_id {
            let user_id = user.id;

            // Revoked means revoked. Destroy the underlying session so the
            // cookie cannot be replayed, rather than merely refusing this one
            // request.
            if state.service.is_session_revoked(&sid).await {
                let _ = auth.logout().await;
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    "This session has been signed out",
                )
                    .into_response();
            }

            let ua = request
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_owned());
            let ip = request
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|s| s.trim().to_owned());

            if let Err(e) = state
                .service
                .touch_session(&sid, user_id, ua.as_deref(), ip.as_deref())
                .await
            {
                tracing::debug!("session touch failed: {e}");
            }
        }
    }

    next.run(request).await
}
