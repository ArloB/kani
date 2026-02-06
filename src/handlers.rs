use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde_json::json;

use crate::{
    error::AppError,
    models::{CreateSource, FetchWasmRequest, Source, UpdateSource},
    state::AppState,
};

pub async fn list_sources(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let sources = sqlx::query_as!(Source, "SELECT id, name, version FROM sources LIMIT 1000")
        .fetch_all(&state.db)
        .await?;

    Ok((StatusCode::OK, Json(sources)))
}

pub async fn add_source(
    State(state): State<AppState>,
    Json(payload): Json<CreateSource>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query!(
        "INSERT INTO sources (name, version) VALUES (?, '0.1') RETURNING id",
        payload.name
    )
    .fetch_one(&state.db)
    .await?;

    Ok((StatusCode::CREATED, Json(json!({ "id": result.id }))))
}

pub async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    Ok((StatusCode::OK, Json(source)))
}

pub async fn update_source(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateSource>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_none() && payload.version.is_none() {
        return Ok((StatusCode::OK, Json(json!({}))));
    }

    let mut builder = sqlx::QueryBuilder::new("UPDATE sources SET ");
    let mut separated = builder.separated(", ");

    if let Some(name) = payload.name {
        separated.push("name = ");
        separated.push_bind_unseparated(name);
    }

    if let Some(version) = payload.version {
        separated.push("version = ");
        separated.push_bind_unseparated(version);
    }

    builder.push(" WHERE id = ");
    builder.push_bind(id);

    builder.build().execute(&state.db).await?;

    Ok((StatusCode::OK, Json(json!({}))))
}

pub async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query!("DELETE FROM sources WHERE id = ? RETURNING name", id)
        .fetch_optional(&state.db)
        .await?;

    if let Some(row) = result {
        let settings = state.settings.read().await;
        kani_core::file_storage::delete_wasm_file(&settings.wasm_storage_path, &row.name)
            .await
            .map_err(|e| AppError::CoreError(e))?;
    }

    Ok((StatusCode::OK, Json(json!({}))))
}

/// Upload a WASM binary via multipart form data
pub async fn upload_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    // Verify source exists
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    if source.is_none() {
        return Err(AppError::NotFound(format!("Source {} not found", id)));
    }

    // Extract the file from multipart
    let mut wasm_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            wasm_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        AppError::InternalServerError(format!("Failed to read file: {}", e))
                    })?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = wasm_bytes
        .ok_or_else(|| AppError::InternalServerError("No file field in multipart".to_string()))?;

    // Save the WASM file
    let name = source
        .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?
        .name;
    let settings = state.settings.read().await;
    let path = kani_core::file_storage::save_wasm(&settings.wasm_storage_path, &name, &bytes)
        .await
        .map_err(|e| AppError::CoreError(e))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

/// Fetch a WASM binary from an external URL
pub async fn fetch_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify source exists
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    if source.is_none() {
        return Err(AppError::NotFound(format!("Source {} not found", id)));
    }

    // Fetch the WASM from the URL
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| AppError::InternalServerError(format!("Failed to create client: {}", e)))?;

    let response = client
        .get(&payload.url)
        .send()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch: {}", e)))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to read response: {}", e)))?;

    // Save the WASM file
    let name = source
        .ok_or_else(|| AppError::NotFound(format!("Source {} not found", id)))?
        .name;
    let settings = state.settings.read().await;
    let path = kani_core::file_storage::save_wasm(&settings.wasm_storage_path, &name, &bytes)
        .await
        .map_err(|e| AppError::CoreError(e))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

pub async fn get_popular_manga(
    State(state): State<AppState>,
    Path((id, page)): Path<(i64, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_popular_manga(id, page).await?;

    Ok((StatusCode::OK, Json(result)))
}

pub async fn search_manga(
    State(state): State<AppState>,
    Path((id, query, page)): Path<(i64, String, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.search_manga(id, &query, page).await?;

    Ok((StatusCode::OK, Json(result)))
}

pub async fn get_manga_details(
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_manga_details(id, manga_id).await?;

    Ok((StatusCode::OK, Json(result)))
}

pub async fn start_download(
    State(state): State<AppState>,
    Path((id, manga_id, chapter_id)): Path<(i64, i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    state.start_download(id, manga_id, chapter_id).await?;

    Ok((StatusCode::OK, Json(json!({}))))
}
