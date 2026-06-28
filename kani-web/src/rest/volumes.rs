use super::*;
use kani_app::ids::ChapterId;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/manga/{id}/volumes", get(list_volumes).post(create_volume))
        .route(
            "/manga/{id}/volumes/{vid}",
            put(update_volume).delete(delete_volume),
        )
        .route(
            "/manga/{id}/chapters/{cid}/volume",
            put(assign_chapter_volume),
        )
}

#[derive(serde::Deserialize)]
pub(crate) struct VolumeBody {
    pub name: Option<String>,
    pub volume_num: Option<f64>,
}

#[derive(serde::Deserialize)]
pub(crate) struct AssignVolumeBody {
    pub volume_id: Option<i64>,
}

pub(crate) async fn list_volumes(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.service.list_volumes(manga_id).await?))
}

pub(crate) async fn create_volume(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<VolumeBody>,
) -> Result<impl IntoResponse, AppError> {
    let vol = state
        .service
        .create_volume(manga_id, body.name, body.volume_num)
        .await?;
    Ok((StatusCode::CREATED, Json(vol)))
}

pub(crate) async fn update_volume(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path((manga_id, volume_id)): Path<(MangaId, i64)>,
    Json(body): Json<VolumeBody>,
) -> Result<impl IntoResponse, AppError> {
    let vol = state
        .service
        .update_volume(volume_id, manga_id, body.name, body.volume_num)
        .await?;
    Ok(Json(vol))
}

pub(crate) async fn delete_volume(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path((manga_id, volume_id)): Path<(MangaId, i64)>,
) -> Result<impl IntoResponse, AppError> {
    state.service.delete_volume(volume_id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn assign_chapter_volume(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path((manga_id, chapter_id)): Path<(MangaId, ChapterId)>,
    Json(body): Json<AssignVolumeBody>,
) -> Result<impl IntoResponse, AppError> {
    state
        .service
        .assign_chapter_volume(chapter_id, manga_id, body.volume_id)
        .await?;
    Ok(Json(json!({})))
}
