//! Server settings, auto-scan toggle & refresh routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/scan/toggle_auto", post(toggle_auto_scan))
        .route("/refresh/start", post(start_refresh_all_rest))
        .route("/refresh/status", get(get_refresh_status))
}

async fn get_settings(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_settings().await))
}

async fn update_settings(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(update): Json<crate::types::SettingsUpdate>,
) -> Result<impl IntoResponse, AppError> {
    use crate::types::SettingsUpdate;

    let user = auth
        .user
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
    let required_perm = match &update {
        SettingsUpdate::Download(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditDownload)
        }
        SettingsUpdate::Scan(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditScan)
        }
        SettingsUpdate::Advanced(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditAdvanced)
        }
        SettingsUpdate::Tracking(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditScan)
        }
        SettingsUpdate::Email(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditAdvanced)
        }
    };
    if !auth
        .backend
        .has_perm(&user, required_perm)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
    {
        return Err(AppError::Forbidden("Insufficient permissions".into()));
    }
    if let crate::types::SettingsUpdate::Advanced(ref adv) = update {
        crate::HTTP_LOGGING_ENABLED.store(
            adv.http_request_logging,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    state.update_settings(update, user.id).await?;
    Ok(Json(json!({})))
}

async fn toggle_auto_scan(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditScan>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let new_val = state.toggle_auto_scan().await?;
    Ok(Json(json!({ "auto_scan": new_val })))
}

pub async fn start_refresh_all_rest(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.start_refresh_all().await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn get_refresh_status(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        json!({ "is_refreshing": state.is_refreshing().await }),
    ))
}
