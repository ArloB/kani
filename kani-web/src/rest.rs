//! Plain Axum REST handlers — mounted at /api in main.rs.

use axum::{
    Json, Router,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, post},
};
use serde_json::json;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use crate::{
    error::AppError,
    models::{CreateSource, FetchWasmRequest, Manga, ProxyQuery, SearchMangaRequest, Source, UpdateSource},
    state::AppState,
};
use kani_core::source_manager::SourceManager;
use kani_shared::{ChapterList, MangaInfo};

pub fn routes(state: AppState) -> Router {
    Router::new()
        // Binary response — image proxy
        .route("/image_proxy", get(image_proxy))
        // SSE — download progress
        .route("/downloads/progress", get(download_progress_sse))
        // Source admin CRUD
        .route("/sources", get(list_sources).post(add_source))
        .route(
            "/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/sources/{id}/metadata", get(get_metadata))
        // WASM installation (multipart + URL fetch)
        .route("/sources/{id}/wasm", post(upload_wasm))
        .route("/sources/{id}/wasm/fetch", post(fetch_wasm))
        // Source data endpoints
        .route("/sources/{id}/popular/{page}", get(get_popular_manga))
        .route("/sources/{id}/search/{page}", get(search_manga))
        .route("/sources/{id}/details/{manga_id}", get(get_manga_details))
        .route("/sources/{id}/save/{manga_id}", post(save_to_library))
        .route(
            "/sources/{id}/chapters/{manga_id}/{page}",
            get(get_chapter_list),
        )
        // Library management
        .route("/library/{page}/{order}", get(get_library))
        .route("/manga/{id}", get(get_manga).delete(delete_manga))
        .route("/chapter/{id}/download", post(start_download))
        .route("/chapter/{id}/delete", delete(delete_downloaded))
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// Download progress SSE
// ─────────────────────────────────────────────────────────────────────────────

pub async fn download_progress_sse(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rx = state.downloader.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("SSE client lagged, skipped {} download progress events", n);
            None
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Image proxy
// ─────────────────────────────────────────────────────────────────────────────

pub async fn image_proxy(
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> Result<impl IntoResponse, AppError> {
    if kani_core::network::is_private_host(&query.url) {
        return Err(AppError::InternalServerError(
            "Proxy request blocked: target is a private or reserved address".to_string(),
        ));
    }

    let response = state
        .smart_client
        .inner()
        .get(&query.url)
        .header("Referer", &query.referer)
        .send()
        .await?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .ok_or_else(|| AppError::InternalServerError("Failed to get content type".to_string()))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok((
        status,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        bytes,
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Source CRUD
// ─────────────────────────────────────────────────────────────────────────────

async fn list_sources(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let sources = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources LIMIT 1000"
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(sources))
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

    Ok(Json(source))
}

async fn update_source(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateSource>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_none() && payload.version.is_none() {
        return Ok((StatusCode::OK, Json(json!({}))));
    }

    sqlx::query!(
        "UPDATE sources SET name = COALESCE(?, name), version = COALESCE(?, version) WHERE id = ?",
        payload.name,
        payload.version,
        id
    )
    .execute(&state.db)
    .await?;

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

async fn get_metadata(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_metadata(id).await?;
    Ok(result)
}

// ─────────────────────────────────────────────────────────────────────────────
// WASM installation helper + handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn install_source(
    state: &AppState,
    id: i64,
    current_source: &Source,
    bytes: &[u8],
) -> Result<std::path::PathBuf, AppError> {
    let component = match state.wasm_runtime.compile_component(bytes) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to compile component: {}", e);
            return Err(AppError::CoreError(e));
        }
    };

    let metadata = {
        let mut inst = kani_core::sources::SourceInstance::new(state.smart_client.clone(), None);
        inst.load(
            state.wasm_runtime.engine(),
            &component,
            state.wasm_runtime.linker(),
        )
        .await
        .map_err(AppError::CoreError)?;
        inst.get_metadata().await.map_err(AppError::CoreError)?
    };

    sqlx::query!(
        "UPDATE sources SET name = ?, version = ?, base_url = ? WHERE id = ?",
        metadata.name,
        metadata.version,
        metadata.base_url,
        id
    )
    .execute(&state.db)
    .await?;

    let settings = state.settings.read().await;
    let storage_path = settings
        .wasm_storage_path
        .to_str()
        .ok_or_else(|| AppError::InternalServerError("Failed to convert path".to_string()))?;

    if current_source.name != metadata.name {
        tracing::info!(
            "Source name changed from {} to {}. Deleting old file.",
            current_source.name,
            metadata.name
        );
        let _ =
            kani_core::file_storage::delete_wasm_file(storage_path, &current_source.name).await;
    }

    let path = kani_core::file_storage::save_wasm(storage_path, &metadata.name, bytes)
        .await
        .map_err(AppError::CoreError)?;
    drop(settings);

    let source_manager = SourceManager::new(
        state.wasm_runtime.engine().clone(),
        component,
        state.wasm_runtime.linker().clone(),
        state.smart_client.clone(),
        Some(metadata.base_url.clone()),
        25,
        1,
    )
    .await
    .map_err(AppError::CoreError)?;

    state
        .sources
        .write()
        .await
        .insert(id, Arc::new(source_manager));

    tracing::info!(
        "Successfully installed source {}: {} v{}",
        id,
        metadata.name,
        metadata.version
    );

    Ok(path)
}

async fn upload_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

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

    let path = install_source(&state, id, &source, &bytes).await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

async fn fetch_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    let source = sqlx::query_as!(
        Source,
        "SELECT id, name, version, base_url FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

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

    let path = install_source(&state, id, &source, &bytes).await?;

    Ok((
        StatusCode::OK,
        Json(json!({ "path": path.to_string_lossy() })),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Source data handlers (thin delegation to AppState)
// ─────────────────────────────────────────────────────────────────────────────

async fn get_popular_manga(
    State(state): State<AppState>,
    Path((id, page)): Path<(i64, i32)>,
) -> Result<impl IntoResponse, AppError> {
    state.get_popular_manga(id, page).await
}

async fn search_manga(
    State(state): State<AppState>,
    Path((id, page)): Path<(i64, i32)>,
    Query(payload): Query<SearchMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.search_manga(id, &payload.query, page).await
}

async fn get_manga_details(
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    state.get_manga_details(id, &manga_id).await
}

async fn get_chapter_list(
    State(state): State<AppState>,
    Path((id, manga_id, page)): Path<(i64, String, i32)>,
) -> Result<impl IntoResponse, AppError> {
    state.get_chapter_list_paged(id, &manga_id, page).await
}

async fn save_to_library(
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let exists: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
        manga_id,
        id
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    let result = state.get_manga_details(id, &manga_id).await?;
    let chapters = state.get_chapter_list(id, &manga_id).await?;

    let manga = serde_json::from_str::<MangaInfo>(&result)
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse manga: {e}")))?;

    let chapter = serde_json::from_str::<ChapterList>(&chapters)
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse chapter: {e}")))?;

    let id = if exists.is_none() {
        let mut tx = state.db.begin().await?;

        let status: i64 = manga.status.into();

        let result = sqlx::query!(
            "INSERT INTO manga (source_manga_id, source_id, name, cover_url, description, status) \
            VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
            manga.id,
            id,
            manga.title,
            manga.cover_url,
            manga.description,
            status
        )
        .fetch_one(&mut *tx)
        .await?;

        let manga_row_id = result
            .id
            .ok_or_else(|| AppError::InternalServerError("Failed to get manga id".to_string()))?;

        for author in &manga.authors {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", author)
                .execute(&mut *tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_authors (manga_id, person_id) \
                SELECT ?, id FROM people WHERE name = ?",
                manga_row_id,
                author
            )
            .execute(&mut *tx)
            .await?;
        }

        for artist in &manga.artists {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", artist)
                .execute(&mut *tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_artists (manga_id, person_id) \
                SELECT ?, id FROM people WHERE name = ?",
                manga_row_id,
                artist
            )
            .execute(&mut *tx)
            .await?;
        }

        for tag in &manga.tags {
            sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", tag)
                .execute(&mut *tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
                SELECT ?, id FROM tags WHERE name = ?",
                manga_row_id,
                tag
            )
            .execute(&mut *tx)
            .await?;
        }

        for c in chapter.chapters {
            sqlx::query!(
                "INSERT INTO chapters (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                manga_row_id,
                c.id,
                c.title,
                c.number,
                c.language,
                c.volume,
                c.scanlator,
                c.date_uploaded
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        manga_row_id
    } else if let Some(existing_id) = exists {
        existing_id
    } else {
        return Err(AppError::InternalServerError(
            "Failed to get manga id".to_string(),
        ));
    };

    Ok(Json(json!(id)))
}

// ─────────────────────────────────────────────────────────────────────────────
// Library management handlers
// ─────────────────────────────────────────────────────────────────────────────

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

async fn start_download(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.start_download(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn delete_downloaded(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_downloaded(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}