//! Server settings, auto-scan toggle & refresh routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/scan/toggle_auto", post(toggle_auto_scan))
        .route("/refresh/start", post(start_refresh_all_rest))
        .route("/refresh/status", get(get_refresh_status))
}

#[utoipa::path(
    get, path = "/rest/settings",
    responses(
        (status = 200, description = "Server settings (download, scan, advanced, tracking, email)"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Insufficient permissions"),
    ),
    security(("session" = [])),
    tag = "system"
)]
pub(crate) async fn get_settings(
    _: AuthGuard<crate::permissions::guards::SettingsView>,
    State(svc): State<Arc<dyn SettingsDomain>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_settings().await))
}

#[utoipa::path(
    patch, path = "/rest/settings",
    request_body(content = inline(serde_json::Value), description = "Partial settings update; variant determines which permission is required"),
    responses(
        (status = 200, description = "Settings updated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Insufficient permissions for the given settings category"),
    ),
    security(("session" = [])),
    tag = "system"
)]
pub(crate) async fn update_settings(
    auth: AuthSession,
    State(svc): State<Arc<dyn SettingsDomain>>,
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
    svc.update_settings(update, user.id).await?;
    Ok(Json(json!({})))
}

#[utoipa::path(
    post, path = "/rest/scan/toggle_auto",
    responses(
        (status = 200, description = "Auto-scan toggled; returns new state"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Insufficient permissions"),
    ),
    security(("session" = [])),
    tag = "system"
)]
pub(crate) async fn toggle_auto_scan(
    _: AuthGuard<crate::permissions::guards::SettingsEditScan>,
    State(svc): State<Arc<dyn SettingsDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let new_val = svc.toggle_auto_scan().await?;
    Ok(Json(json!({ "auto_scan": new_val })))
}

#[utoipa::path(
    post, path = "/rest/refresh/start",
    responses(
        (status = 202, description = "Library metadata refresh started"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Insufficient permissions"),
    ),
    security(("session" = [])),
    tag = "library"
)]
pub async fn start_refresh_all_rest(
    _: AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(svc): State<Arc<dyn SettingsDomain>>,
) -> Result<impl IntoResponse, AppError> {
    svc.start_refresh_all().await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

#[utoipa::path(
    get, path = "/rest/refresh/status",
    responses(
        (status = 200, description = "Whether a metadata refresh is currently running"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "library"
)]
pub(crate) async fn get_refresh_status(
    _: AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(svc): State<Arc<dyn SettingsDomain>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(json!({ "is_refreshing": svc.is_refreshing().await })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn stub_user() -> crate::auth::User {
        crate::auth::User {
            id: UserId(1),
            username: "stub".into(),
            email: "stub@test.com".into(),
            is_active: true,
            roles: vec![],
            password_hash: String::new(),
            change_id: vec![],
        }
    }

    struct StubSettings;

    #[async_trait::async_trait]
    impl SettingsDomain for StubSettings {
        async fn get_settings(&self) -> kani_shared::types::AppSettings {
            unimplemented!()
        }
        async fn update_settings(
            &self,
            _update: kani_shared::types::SettingsUpdate,
            _user_id: UserId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn toggle_auto_scan(&self) -> kani_app::error::Result<bool> {
            Ok(false)
        }
        async fn start_refresh_all(&self) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn is_refreshing(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn toggle_auto_scan_returns_ok_without_appservice() {
        let svc: Arc<dyn SettingsDomain> = Arc::new(StubSettings);
        let response = toggle_auto_scan(AuthGuard(stub_user(), PhantomData), State(svc))
            .await
            .unwrap();
        let resp = axum::response::IntoResponse::into_response(response);
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }
}
