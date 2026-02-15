use axum::{
    Json, Router,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::json;

use crate::{
    error::AppError,
    etag::{etag_bytes_response, etag_json_response},
    models::{CreateSource, FetchWasmRequest, SearchMangaRequest, Source, UpdateSource},
    state::AppState,
};
use kani_core::sources::SourceHost;

async fn list_sources(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let sources = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources LIMIT 1000"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(etag_json_response(&headers, &sources)?)
}

async fn add_source(
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

async fn get_source(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    Ok(etag_json_response(&headers, &source)?)
}

async fn update_source(
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

async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query!("DELETE FROM sources WHERE id = ? RETURNING name", id)
        .fetch_optional(&state.db)
        .await?;

    if let Some(row) = result {
        let settings = state.settings.read().await;
        kani_core::file_storage::delete_wasm_file(
            settings.wasm_storage_path.to_str().ok_or_else(|| {
                AppError::InternalServerError("Failed to convert path to string".to_string())
            })?,
            &row.name,
        )
        .await
        .map_err(AppError::CoreError)?;

        drop(settings);
    }

    Ok((StatusCode::OK, Json(json!({}))))
}

/// Upload a WASM binary via multipart form data
async fn upload_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    // Verify source exists
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    if source.is_none() {
        return Err(AppError::NotFound(format!("Source {id} not found")));
    }

    // Extract the file from multipart
    let mut wasm_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            wasm_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        AppError::InternalServerError(format!("Failed to read file: {e}"))
                    })?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = wasm_bytes
        .ok_or_else(|| AppError::InternalServerError("No file field in multipart".to_string()))?;

    // Save the WASM file
    let name = &source
        .as_ref()
        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
        .name;
    let settings = state.settings.read().await;
    let path = kani_core::file_storage::save_wasm(
        settings.wasm_storage_path.to_str().ok_or_else(|| {
            AppError::InternalServerError("Failed to convert path to string".to_string())
        })?,
        name,
        &bytes,
    )
    .await
    .map_err(AppError::CoreError)?;

    match SourceHost::new(
        Some(state.settings.read().await.flaresolverr_url.clone()),
        name,
    )
    .load(state.wasm_runtime.engine(), &settings.wasm_storage_path)
    .await
    {
        Ok(host) => {
            state.sources.lock().await.insert(id, host);
            tracing::info!("Successfully loaded source: {}", name);
        }
        Err(e) => {
            tracing::error!("Failed to load source {}: {}", name, e);
        }
    }

    drop(settings);

    let metadata = state.get_metadata(id).await?;

    // Update the source with metadata
    sqlx::query!(
        "UPDATE sources SET base_url = ? WHERE id = ?",
        metadata.base_url,
        id
    )
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

/// Fetch a WASM binary from an external URL
async fn fetch_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify source exists
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    if source.is_none() {
        return Err(AppError::NotFound(format!("Source {id} not found")));
    }

    // Fetch the WASM from the URL
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| AppError::InternalServerError(format!("Failed to create client: {e}")))?;

    let response = client
        .get(&payload.url)
        .send()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to fetch: {e}")))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::InternalServerError(format!("Failed to read response: {e}")))?;

    // Save the WASM file
    let name = source
        .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?
        .name;
    let settings = state.settings.read().await;
    let path = kani_core::file_storage::save_wasm(
        settings.wasm_storage_path.to_str().ok_or_else(|| {
            AppError::InternalServerError("Failed to convert path to string".to_string())
        })?,
        &name,
        &bytes,
    )
    .await
    .map_err(AppError::CoreError)?;

    match SourceHost::new(
        Some(state.settings.read().await.flaresolverr_url.clone()),
        &name,
    )
    .load(state.wasm_runtime.engine(), &settings.wasm_storage_path)
    .await
    {
        Ok(host) => {
            state.sources.lock().await.insert(id, host);
            tracing::info!("Successfully loaded source: {}", name);
        }
        Err(e) => {
            tracing::error!("Failed to load source {}: {}", name, e);
        }
    }

    drop(settings);

    let metadata = state.get_metadata(id).await?;

    // Update the source with metadata
    sqlx::query!(
        "UPDATE sources SET base_url = ? WHERE id = ?",
        metadata.base_url,
        id
    )
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

async fn get_popular_manga(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((id, page)): Path<(i64, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_popular_manga(id, page).await?;

    Ok(etag_bytes_response(&headers, &result))
}

async fn search_manga(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((id, page)): Path<(i64, i32)>,
    Query(payload): Query<SearchMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.search_manga(id, &payload.query, page).await?;

    Ok(etag_bytes_response(&headers, &result))
}

async fn get_manga_details(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_manga_details(id, &manga_id).await?;

    Ok(etag_bytes_response(&headers, &result))
}

async fn get_chapter_list(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((id, manga_id, page)): Path<(i64, String, i32)>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_chapter_list(id, &manga_id, page).await?;

    Ok(etag_bytes_response(&headers, &result))
}

async fn start_download(
    State(state): State<AppState>,
    Path((id, manga_id, chapter_id)): Path<(i64, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    state.start_download(id, &manga_id, &chapter_id).await?;

    Ok((StatusCode::OK, Json(json!({}))))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources).post(add_source))
        .route(
            "/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/sources/{id}/wasm", post(upload_wasm))
        .route("/sources/{id}/wasm/fetch", post(fetch_wasm))
        .route("/sources/{id}/popular/{page}", get(get_popular_manga))
        .route("/sources/{id}/search/{page}", get(search_manga))
        .route("/sources/{id}/details/{manga_id}", get(get_manga_details))
        .route(
            "/sources/{id}/chapters/{manga_id}/{page}",
            get(get_chapter_list),
        )
        .route(
            "/sources/{id}/download/{manga_id}/{chapter_id}",
            post(start_download),
        )
}
