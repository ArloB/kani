//! Per-chapter reading, progress, bookmark & note routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chapter/{id}/pages", get(get_chapter_page_manifest))
        .route("/chapter/{id}/progress", put(set_chapter_progress_handler))
        .route(
            "/chapter/{id}/bookmarks",
            get(get_bookmarks_handler).post(toggle_bookmark_handler),
        )
        .route(
            "/chapter/{id}/note",
            get(get_chapter_note_handler).put(set_chapter_note_handler),
        )
        .route(
            "/manga/{id}/chapter-notes",
            get(get_manga_chapter_notes_handler),
        )
        .route(
            "/chapters/read_status",
            put(set_chapter_read_status_handler),
        )
        .route(
            "/manga/{id}/continue_reading",
            get(get_continue_reading_handler),
        )
        .route(
            "/manga/{id}/chapters/mark_up_to",
            post(mark_chapters_up_to_handler),
        )
}

async fn get_chapter_page_manifest(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let manifest = state.get_chapter_page_manifest(id, user.id).await?;
    Ok(Json(manifest))
}

async fn set_chapter_progress_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
    Json(body): Json<SetChapterProgressRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_chapter_progress(user.id, chapter_id, body.page)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_bookmarks_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let pages = state.get_bookmarks(user.id, chapter_id).await?;
    Ok(Json(pages))
}

async fn toggle_bookmark_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
    Json(body): Json<crate::models::ToggleBookmarkRequest>,
) -> Result<impl IntoResponse, AppError> {
    let bookmarked = state
        .toggle_bookmark(user.id, chapter_id, body.page_index)
        .await?;
    Ok(Json(json!({ "bookmarked": bookmarked })))
}

async fn get_chapter_note_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let note = state.get_chapter_note(user.id, chapter_id).await?;
    Ok(Json(json!({ "note": note })))
}

async fn set_chapter_note_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
    Json(body): Json<crate::models::SetChapterNoteRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_chapter_note(user.id, chapter_id, &body.note)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_manga_chapter_notes_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let notes = state
        .get_manga_chapter_notes_with_text(user.id, manga_id)
        .await?;
    let items: Vec<_> = notes
        .into_iter()
        .map(|(chapter_id, chapter_number, note)| {
            json!({ "chapter_id": chapter_id, "chapter_number": chapter_number, "note": note })
        })
        .collect();
    Ok(Json(json!({ "notes": items })))
}

async fn set_chapter_read_status_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Json(body): Json<SetReadStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_chapter_read_status(user.id, body.chapter_ids, body.is_read)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_continue_reading_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let info = state
        .get_continue_reading_chapter(user.id, manga_id)
        .await?;
    Ok(Json(info))
}

async fn mark_chapters_up_to_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<MarkUpToRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ids = state
        .get_chapters_up_to(manga_id, body.chapter_number)
        .await?;
    state
        .set_chapter_read_status(user.id, ids, body.is_read)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
