//! Tower middleware that calls `AppService::touch_session` after axum-login
//! has populated the `AuthSession` for authenticated requests.
//!
//! This maintains the `user_sessions` sidecar table used by the session
//! inventory UI.

use axum::{
    body::Body,
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_login::AuthSession;

use crate::{auth::AuthBackend, state::AppState};

/// Middleware: if the request is authenticated, record a session touch.
/// Passes through unconditionally — errors are non-fatal.
pub async fn session_touch_middleware(
    State(state): State<AppState>,
    auth: AuthSession<AuthBackend>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(user) = &auth.user {
        // Extract the tower-sessions session ID from the cookie header so we can
        // correlate with the user_sessions table.
        let session_id = auth.session.id().map(|id| id.to_string());
        if let Some(sid) = session_id {
            let user_id = user.id;
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
