//! Scanlator preference & per-manga scanlator/language routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/manga/{id}/scanlator_preferences",
            get(get_scanlator_prefs).post(set_scanlator_pref),
        )
        .route("/scanlator_preferences/{id}", delete(delete_scanlator_pref))
        .route(
            "/manga/{id}/scanlator_mode",
            patch(set_scanlator_mode_handler),
        )
        .route("/manga/{id}/scanlators", get(get_chapter_scanlators))
        .route("/manga/{id}/languages", get(get_chapter_languages))
}

async fn get_scanlator_prefs(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_scanlator_prefs(manga_id).await?))
}

async fn set_scanlator_pref(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetScanlatorPrefRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_scanlator_pref(manga_id, &body.scanlator, body.priority, body.blocked)
        .await?;
    Ok(Json(json!({})))
}

async fn delete_scanlator_pref(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(pref_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_scanlator_pref(pref_id).await?;
    Ok(Json(json!({})))
}

async fn set_scanlator_mode_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<crate::models::SetScanlatorModeRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.set_scanlator_mode(manga_id, &body.mode).await?;
    Ok(Json(json!({})))
}

async fn get_chapter_scanlators(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_chapter_scanlators(manga_id).await?))
}

async fn get_chapter_languages(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_chapter_languages(manga_id).await?))
}
