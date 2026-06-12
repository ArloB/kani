//! Webhook CRUD, delivery & per-manga notify routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/webhooks", get(list_webhooks).post(create_webhook))
        .route(
            "/webhooks/{id}",
            patch(update_webhook).delete(delete_webhook),
        )
        .route("/webhooks/{id}/test", post(test_webhook))
        .route("/webhooks/{id}/deliveries", get(list_webhook_deliveries))
        .route(
            "/manga/{id}/webhook-notify",
            get(get_manga_webhook_notify).put(set_manga_webhook_notify),
        )
}

async fn list_webhooks(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let webhooks = state.webhook_service.list_webhooks().await?;
    Ok(Json(webhooks))
}

async fn create_webhook(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<kani_app::service::webhooks::CreateWebhookBody>,
) -> Result<impl IntoResponse, AppError> {
    let webhook = state.webhook_service.create_webhook(body).await?;
    Ok((StatusCode::CREATED, Json(webhook)))
}

async fn update_webhook(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<kani_app::service::webhooks::UpdateWebhookBody>,
) -> Result<impl IntoResponse, AppError> {
    let webhook = state.webhook_service.update_webhook(id, body).await?;
    Ok(Json(webhook))
}

async fn delete_webhook(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.webhook_service.delete_webhook(id).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn test_webhook(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    match state.webhook_service.send_test(id).await {
        Ok(()) => Ok(Json(json!({ "ok": true }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

async fn list_webhook_deliveries(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state.webhook_service.list_deliveries(id).await?;
    Ok(Json(rows))
}

async fn get_manga_webhook_notify(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    let enabled = state.webhook_service.get_manga_notify(id).await?;
    Ok(Json(json!({ "enabled": enabled })))
}

async fn set_manga_webhook_notify(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(id): Path<MangaId>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let enabled = body
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| AppError::ValidationError("Missing 'enabled' boolean field".into()))?;
    state.webhook_service.set_manga_notify(id, enabled).await?;
    Ok(Json(json!({ "ok": true })))
}
