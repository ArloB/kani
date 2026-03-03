use axum::{
    Json, Router,
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get, post},
};
use rquest::StatusCode;
use serde_json::json;

use crate::error::AppError;
use crate::models::Manga;
use crate::state::AppState;

async fn get_library(
    State(state): State<AppState>,
    Path((page, order)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let order = match order {
        1 => "id DESC",
        _ => "id ASC",
    };

    let offset = (page - 1).max(0) * 20;

    let manga = sqlx::query_as!(
        Manga,
        "SELECT * FROM manga ORDER BY ? LIMIT ? OFFSET ?",
        order,
        20,
        offset
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(manga))
}

async fn get_manga(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let manga = sqlx::query_as!(Manga, "SELECT * FROM manga WHERE id = ?", id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("No manga found with id {id}")))?;

    Ok(Json(manga))
}

async fn delete_manga(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query!("DELETE FROM manga WHERE id = ?", id)
        .execute(&state.db)
        .await?;

    Ok(Json(json!({})))
}

// async fn get_chapters(
//     State(state): State<AppState>,
//     Path(id): Path<i64>,
// ) -> Result<impl IntoResponse, AppError> {
//     let chapters = sqlx::query_as!(Chapter, "SELECT * FROM chapters WHERE manga_id = ?", id)
//         .fetch_all(&state.db)
//         .await?;
//
//     Ok(Json(chapters))
// }

async fn start_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.start_download(id).await?;

    Ok((StatusCode::OK, Json(json!({}))))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/library/{page}/{order}", get(get_library))
        .route("/manga/{id}", get(get_manga))
        .route("/manga/{id}", delete(delete_manga))
        .route("/chapter/{id}/download", post(start_download))
}
