use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system/info", get(system_info))
        .route("/system/first-run-complete", post(complete_first_run))
}

#[utoipa::path(
    get,
    path = "/rest/system/info",
    responses(
        (status = 200, description = "System information",
         body = inline(serde_json::Value),
         example = json!({
             "version": "0.1.0",
             "first_run": false,
             "oidc_available": false,
             "registration_enabled": true
         }))
    ),
    tag = "system"
)]
pub(crate) async fn system_info(State(state): State<AppState>) -> impl IntoResponse {
    let s = state.get_settings().await;
    Json(json!({
        "version": crate::KANI_VERSION,
        "first_run": !s.first_run_complete,
        "oidc_available": std::env::var("KANI_OIDC_ISSUER").is_ok(),
        "registration_enabled": s.registration_enabled,
    }))
}

#[utoipa::path(
    post, path = "/rest/system/first-run-complete",
    responses(
        (status = 204, description = "First-run flag cleared"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "system"
)]
pub(crate) async fn complete_first_run(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<impl IntoResponse, AppError> {
    let user_id = auth
        .user
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?
        .id;
    state.mark_first_run_complete(user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
