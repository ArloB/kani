//! Plain Axum REST handlers — mounted at /api in main.rs.

use axum::{
    Json, Router, body::Body, extract::{DefaultBodyLimit, Multipart, Path, Query, State}, http::{HeaderMap, StatusCode, header}, response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    }, routing::{delete, get, post}
};
use serde_json::json;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use futures::TryStreamExt;

use crate::{
    error::AppError,
    models::{CreateSource, FetchWasmRequest, Manga, ProxyQuery, SearchMangaRequest, UpdateSource},
    state::AppState, types::Source,
};
use kani_core::source_manager::SourceManager;

const MAX_WASM_BYTES: usize = 10 * 1024 * 1024;

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
        .route(
            "/sources/{id}/pages/{manga_id}/{chapter_id}",
            get(get_pages),
        )
        // Library management
        .route("/library/{page}/{order}", get(get_library))
        .route("/manga/{id}", get(get_manga).delete(delete_manga))
        .route("/manga/{id}/cover", get(serve_manga_cover))
        .route("/chapter/{id}/download", post(start_download))
        .route("/chapter/{id}/delete", delete(delete_downloaded))
        // Events
        .route("/events", get(combined_sse))
        .route("/refresh/start", post(start_refresh_all_rest))
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_WASM_BYTES))
}

pub async fn download_progress_sse(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rx = state.downloader.subscribe();
    let snapshot    = state.downloader.snapshot().await;

    let snapshot_event = {
        let json = serde_json::json!({
            "type": "state_snapshot",
            "chapters": snapshot
        }).to_string();
        Ok::<Event, Infallible>(Event::default().data(json))
    };

    let live_stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("SSE client lagged by {} events, closing to force reconnect", n);
                Some(Ok(Event::default().event("close").data("")))
            }
        }
    });

    let stream = tokio_stream::once(snapshot_event).chain(live_stream);

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

const PER_HOST_CONCURRENCY: usize = 5;

fn host_semaphore(
    map: &dashmap::DashMap<String, Arc<tokio::sync::Semaphore>>,
    host: &str,
) -> Arc<tokio::sync::Semaphore> {
    map.entry(host.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(PER_HOST_CONCURRENCY)))
        .clone()
}

pub async fn image_proxy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProxyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (url, referer) = crate::proxy::unseal_proxy_token(
        &query.token,
        &state.proxy_secret,
    )
    .ok_or_else(|| AppError::Other("Invalid or expired proxy token".into()))?;

    let etag = crate::proxy::compute_etag(&url, &referer, &state.proxy_secret);

    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
    && if_none_match.as_bytes() == etag.as_bytes() {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::CACHE_CONTROL, header::HeaderValue::from_static(
                    "public, max-age=31536000, immutable",
                )),
                (header::ETAG, header::HeaderValue::from_str(&etag)
                    .map_err(|e| AppError::InternalServerError(e.to_string()))?),
            ],
            Body::empty(),
        ).into_response());
    }

    let host = rquest::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| url.clone());

    let semaphore = host_semaphore(&state.proxy_semaphores, &host);
    let _permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| AppError::InternalServerError("Semaphore closed".into()))?;

    let mut req_headers = rquest::header::HeaderMap::new();
    req_headers.insert(
        rquest::header::REFERER,
        rquest::header::HeaderValue::from_str(&referer)
            .map_err(AppError::InvalidHeaderValue)?,
    );

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        state.proxy_client.safe_get(&url, Some(req_headers.clone())),
    )
    .await
    .map_err(|_| AppError::Other("Upstream image fetch timed out".into()))??;

    let response = if response.status() == rquest::StatusCode::FORBIDDEN
        || response.status() == rquest::StatusCode::SERVICE_UNAVAILABLE
    {
        let request = state.smart_client
            .inner()
            .get(&url)
            .headers(req_headers)
            .build()
            .map_err(|e| AppError::InternalServerError(e.to_string()))?;

        tokio::time::timeout(
            std::time::Duration::from_secs(90),
            state.smart_client.send_request(request),
        )
        .await
        .map_err(|_| AppError::Other("Upstream image fetch timed out after solver".into()))??
    } else {
        response
    };

    if !response.status().is_success() {
        tracing::warn!("Upstream returned {} for proxied request", response.status().as_u16());
        return Err(AppError::Other(format!(
            "Upstream returned {}",
            response.status().as_u16()
        )));
    }

    let content_type = response
        .headers()
        .get(rquest::header::CONTENT_TYPE)
        .cloned()
        .ok_or_else(|| AppError::InternalServerError(
            "Upstream response missing Content-Type".into(),
        ))?;

    let ct_str = content_type.to_str().unwrap_or("");
    if !ct_str.starts_with("image/") {
        tracing::warn!("Upstream proxy returned non-image Content-Type: {}", ct_str);
        return Err(AppError::Other(format!(
            "Expected image, upstream returned Content-Type: {}",
            ct_str
        )));
    }

    let ct_value = header::HeaderValue::from_bytes(content_type.as_bytes())
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let etag_value = header::HeaderValue::from_str(&etag)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;

    let stream = futures::stream::unfold(
        (response, 0usize, Some(_permit)),
        move |(mut resp, received, permit)| async move {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let new_total = received + chunk.len();
                    if new_total > MAX_IMAGE_BYTES {
                        tracing::warn!(
                            "Upstream image exceeded {} byte limit, aborting stream",
                            MAX_IMAGE_BYTES
                        );
                        None
                    } else {
                        Some((Ok(chunk), (resp, new_total, permit)))
                    }
                }
                Ok(None) => None,
                Err(e) => Some((
                    Err(std::io::Error::other(e.to_string())),
                    (resp, received, permit),
                )),
            }
        },
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ct_value),
            (header::ETAG, etag_value),
            (header::CACHE_CONTROL, header::HeaderValue::from_static(
                "public, max-age=31536000, immutable",
            )),
            (header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff")),
        ],
        Body::from_stream(stream),
    ).into_response())
}

async fn list_sources(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let sources = sqlx::query_as!(
        Source,
        "SELECT * FROM sources LIMIT 1000"
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
    let source = state.get_source(id).await?;
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
        ).await.map_err(AppError::CoreError)?;

        drop(settings);

        state.cache.invalidate_source(id);
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

async fn install_source(
    state: &AppState,
    id: i64,
    current_source: &Source,
    bytes: &[u8],
) -> Result<std::path::PathBuf, AppError> {
    let bytes_owned   = bytes.to_vec();
    let runtime_clone = state.wasm_runtime.clone();

    let component = tokio::task::spawn_blocking(move || {
        runtime_clone.compile_component(&bytes_owned)
    })
    .await
    .map_err(|e| AppError::InternalServerError(
        format!("WASM compilation task panicked: {}", e)
    ))??;

    let (metadata, raw_schema) = {
        let mut inst = kani_core::sources::SourceInstance::new(
            state.smart_client.clone(), None, false
        );
        inst.load(state.wasm_runtime.engine(), &component, state.wasm_runtime.linker())
            .await.map_err(AppError::CoreError)?;
        let meta = inst.get_metadata().await.map_err(AppError::CoreError)?;
        let schema = inst.get_preferences().await.ok();
        (meta, schema)
    };

    sqlx::query!(
        "UPDATE sources SET name = ?, version = ?, base_url = ?, unrestricted_http = ? WHERE id = ?",
        metadata.name,
        metadata.version,
        metadata.base_url,
        metadata.unrestricted_http,
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
        metadata.unrestricted_http,
        25,
        1,
        state.load_pref_map(id).await.unwrap_or_default(),
    )
    .await
    .map_err(AppError::CoreError)?;

    state
        .sources
        .write()
        .await
        .insert(id, Arc::new(source_manager));

    if let Some(raw) = raw_schema {
        let schema: Vec<_> = raw.into_iter().map(Into::into).collect();
        state.cache.insert_preference_schema(id, schema);
    }

    state.cache.invalidate_source(id);

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
        "SELECT * FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::InternalServerError("no file field in upload".into()))?;

    let content_length = field.headers()
        .get(rquest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());

    let bytes: bytes::Bytes = kani_core::http::collect_bytes_limited(
        Box::pin(field.map_err(|e| kani_core::error::Error::Other(e.to_string()))),
        content_length,
        MAX_WASM_BYTES,
    ).await?;

    let _ = install_source(&state, id, &source, bytes.as_ref()).await?;

    Ok(StatusCode::OK)
}

async fn fetch_wasm(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    let source = sqlx::query_as!(
        Source,
        "SELECT * FROM sources WHERE id = ?",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;

    let response = state.proxy_client.safe_get(&payload.url, None).await?;

    let bytes = response.bytes_limited(MAX_WASM_BYTES).await?;

    let _ = install_source(&state, id, &source, &bytes).await?;

    Ok(StatusCode::OK)
}

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

async fn get_pages(
    State(state): State<AppState>,
    Path((id, manga_id, chapter_id)): Path<(i64, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    state.get_pages(id, &manga_id, &chapter_id).await
}

async fn save_to_library(
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let manga_row_id = state.save_to_library(id, &manga_id).await?;
    Ok(Json(json!(manga_row_id)))
}

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
    state.download_chapter(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn delete_downloaded(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_downloaded(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn serve_manga_cover(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let record = sqlx::query!(
        "SELECT local_cover_path FROM manga WHERE id = ?", id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Manga {id} not found")))?;

    let relative = record.local_cover_path
        .ok_or_else(|| AppError::NotFound("No local cover for this manga".into()))?;

    let library_path = state.settings.read().await.library_path.clone();
    let full_path    = library_path.join(&relative);

    let metadata = tokio::fs::metadata(&full_path).await
        .map_err(|_| AppError::NotFound("Cover file not found on disk".into()))?;

    let mtime = metadata.modified()
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis())
        .unwrap_or(0);
    let etag = format!("\"{}\"", mtime);

    if let Some(inm) = headers.get(header::IF_NONE_MATCH)
    && inm.as_bytes() == etag.as_bytes() {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::CACHE_CONTROL, header::HeaderValue::from_static(
                    "public, max-age=31536000, immutable",
                )),
                (header::ETAG, header::HeaderValue::from_str(&etag)
                    .map_err(|e| AppError::InternalServerError(e.to_string()))?),
            ],
            axum::body::Body::empty(),
        ).into_response());
    }

    let ext = full_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let content_type = match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png"          => "image/png",
        "webp"         => "image/webp",
        "gif"          => "image/gif",
        _              => "image/jpeg",
    };

    let bytes = tokio::fs::read(&full_path).await
        .map_err(AppError::IoError)?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE,           header::HeaderValue::from_static(content_type)),
            (header::ETAG,                   header::HeaderValue::from_str(&etag)
                .map_err(|e| AppError::InternalServerError(e.to_string()))?),
            (header::CONTENT_LENGTH,         header::HeaderValue::from(bytes.len())),
            (header::CACHE_CONTROL,          header::HeaderValue::from_static(
                "public, max-age=31536000, immutable",
            )),
            (header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff")),
        ],
        axum::body::Body::from(bytes),
    ).into_response())
}

pub async fn combined_sse(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let snapshot = state.downloader.snapshot().await;
    let is_refreshing = state.is_refreshing().await;

    let snapshot_event = Ok::<Event, Infallible>(Event::default().data(
        serde_json::json!({
            "type": "state_snapshot",
            "chapters": snapshot,
            "is_refreshing": is_refreshing
        }).to_string()
    ));

    let download_rx = state.downloader.subscribe();
    let refresh_rx  = state.subscribe_refresh();

    let download_stream = BroadcastStream::new(download_rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("Download SSE lagged by {} events", n);
                Some(Ok(Event::default().event("close").data("")))
            }
        }
    });

    let refresh_stream = BroadcastStream::new(refresh_rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("Refresh SSE lagged by {} events", n);
                None
            }
        }
    });

    let live_stream = download_stream.merge(refresh_stream);
    let stream = tokio_stream::once(snapshot_event).chain(live_stream);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn start_refresh_all_rest(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.start_refresh_all().await?;
    Ok(StatusCode::ACCEPTED)
}