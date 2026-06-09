//! Chapter download lifecycle & history routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloads/history", get(get_download_history))
        .route("/chapter/{id}/download", post(start_download))
        .route("/chapter/{id}/delete", delete(delete_downloaded))
        .route("/chapter/{id}/cancel", post(cancel_download))
        .route("/downloads/active", delete(cancel_all_global_downloads))
}

async fn get_download_history(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Query(q): Query<DownloadHistoryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.get_download_history(q.limit).await?;
    Ok(Json(items))
}

async fn start_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.download_chapter(id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn delete_downloaded(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_downloaded(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn cancel_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.cancel_download(chapter_id).await?;
    Ok(Json(json!({})))
}

async fn cancel_all_global_downloads(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.cancel_all_global_downloads().await?;
    Ok(Json(json!({ "ok": true })))
}
