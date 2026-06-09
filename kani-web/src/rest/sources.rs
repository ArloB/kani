//! Extension source management & browsing routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources).post(add_source))
        .route("/sources/health", get(get_sources_health))
        .route("/sources/active_ids", get(get_active_source_ids))
        .route(
            "/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/sources/{id}/metadata", get(get_metadata))
        .route("/sources/{id}/wasm", post(upload_wasm))
        .route("/sources/{id}/wasm/fetch", post(fetch_wasm))
        .route("/sources/{id}/reload", post(reload_source_handler))
        .route(
            "/sources/{id}/popular/{page}/{page_size}",
            get(get_popular_manga),
        )
        .route("/sources/{id}/search/{page}/{page_size}", get(search_manga))
        .route("/sources/{id}/details/{manga_id}", get(get_manga_details))
        .route("/sources/{id}/url/{manga_id}", get(get_source_manga_url))
        .route("/sources/{id}/save/{manga_id}", post(save_to_library))
        .route(
            "/sources/{id}/chapters/{manga_id}/{page}/{page_size}",
            get(get_chapter_list),
        )
        .route(
            "/sources/{id}/chapter-sorts/{manga_id}",
            get(get_chapter_sort_list),
        )
        .route(
            "/sources/{id}/pages/{manga_id}/{chapter_id}",
            get(get_pages),
        )
        .route("/sources/{id}/in_library/{manga_id}", get(check_in_library))
        .route("/sources/{id}/toggle_enabled", patch(toggle_source_enabled))
        .route(
            "/sources/{id}/toggle_favourite",
            patch(toggle_source_favourite),
        )
        .route("/sources/{id}/filters", get(get_source_filters))
        .route("/sources/{id}/preference_schema", get(get_pref_schema))
        .route("/sources/{id}/preferences", get(get_source_preferences))
        .route(
            "/sources/{id}/preferences/{key}",
            put(set_source_preference),
        )
        .route(
            "/sources/{id}/preferences/{key}/append",
            post(append_pref_list_item),
        )
        .route(
            "/sources/{id}/preferences/{key}/remove_item",
            post(remove_pref_list_item),
        )
        .route(
            "/sources/{id}/preferences/{key}/toggle_select",
            post(toggle_pref_select_item),
        )
}

async fn list_sources(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.list_sources().await?))
}

async fn add_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<CreateSource>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.add_source(&payload.name, user.id).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn get_sources_health(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_source_health().await?))
}

async fn get_active_source_ids(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let ids: Vec<i64> = state.sources.read().await.keys().copied().collect();
    Ok(Json(ids))
}

async fn get_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;
    Ok(Json(source))
}

async fn update_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ValidatedJson(payload): ValidatedJson<UpdateSource>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_none() && payload.version.is_none() {
        return Ok(Json(json!({})));
    }
    state
        .update_source(id, payload.name, payload.version)
        .await?;
    Ok(Json(json!({})))
}

async fn delete_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_source(id, user.id).await?;
    state
        .audit(
            Some(user.id),
            "source.uninstall",
            None,
            Some(json!({ "source_id": id })),
        )
        .await;
    Ok(Json(json!({})))
}

async fn get_metadata(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_metadata(id).await?;
    Ok(result)
}

async fn upload_wasm(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;

    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::InternalServerError("no file field in upload".into()))?;

    let content_length = field
        .headers()
        .get(rquest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());

    let bytes: bytes::Bytes = kani_core::http::collect_bytes_limited(
        Box::pin(field.map_err(|e| kani_core::error::Error::Other(e.to_string()))),
        content_length,
        MAX_WASM_BYTES,
    )
    .await?;

    let _ = install_source(&state, id, &source, bytes.as_ref()).await?;

    Ok(StatusCode::OK)
}

async fn fetch_wasm(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ValidatedJson(payload): ValidatedJson<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;

    let response = state.proxy_client.safe_get(&payload.url, None).await?;

    let bytes = response.bytes_limited(MAX_WASM_BYTES).await?;

    let _ = install_source(&state, id, &source, &bytes).await?;

    Ok(StatusCode::OK)
}

async fn reload_source_handler(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    reload_source(&state, id).await?;
    Ok(StatusCode::OK)
}

async fn get_popular_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, page, page_size)): Path<(i64, i32, i32)>,
    Query(query): Query<crate::models::PopularMangaQuery>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state
        .get_popular_manga(id, page, page_size, query.filters)
        .await?;
    let mut list: crate::types::MangaList = serde_json::from_str(&json_str)?;
    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state, None));
        }
    }
    Ok(Json(list))
}

async fn search_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, page, page_size)): Path<(i64, i32, i32)>,
    ValidatedQuery(payload): ValidatedQuery<SearchMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state
        .search_manga(
            id,
            &payload.query.unwrap_or("".to_string()),
            page,
            page_size,
            payload.filters,
        )
        .await?;
    let mut list: crate::types::MangaList = serde_json::from_str(&json_str)?;
    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state, None));
        }
    }
    Ok(Json(list))
}

async fn get_manga_details(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state.get_manga_details(id, &manga_id).await?;
    let mut info: crate::types::MangaInfo = serde_json::from_str(&json_str)?;
    info.cover_url = info
        .cover_url
        .map(|url| sign_image_url(&url, &base_url, &state, None));
    info.description_html = info
        .description
        .as_deref()
        .map(crate::utils::render_description);
    Ok(Json(info))
}

async fn get_source_manga_url(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let url = state.get_source_url(id, &manga_id).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

async fn save_to_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryAdd>,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
    Query(q): Query<SaveToLibraryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let manga_row_id = state
        .save_to_library(id, &manga_id, q.force.unwrap_or(false))
        .await?;
    Ok(Json(json!({ "db_id": manga_row_id })))
}

async fn get_chapter_list(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id, page, page_size)): Path<(i64, String, i32, i32)>,
    Query(q): Query<ChapterListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let json_str = state
        .get_chapter_list_paged(id, &manga_id, page, page_size, q.sort)
        .await?;
    let list: crate::types::ChapterList = serde_json::from_str(&json_str)?;
    Ok(Json(list))
}

async fn get_chapter_sort_list(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, _manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let opts = state.get_chapter_sort_list(id).await?;
    Ok(Json(opts))
}

async fn get_pages(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id, chapter_id)): Path<(i64, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state.get_pages(id, &manga_id, &chapter_id).await?;
    let mut contents: crate::types::ChapterContents = serde_json::from_str(&json_str)?;
    for page in &mut contents.pages {
        page.url = sign_image_url(&page.url, &base_url, &state, page.transform.as_deref());
    }
    Ok(Json(contents))
}

async fn check_in_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path((source_id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let decoded = crate::utils::decode_manga_id(&manga_id);
    let db_id = state.check_in_library(source_id, &decoded).await?;
    Ok(Json(json!({ "db_id": db_id })))
}

async fn toggle_source_enabled(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceToggleEnabled>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleEnabledRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.toggle_source_enabled(source_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_source_favourite(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleFavouritedRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_source_favourite(source_id, body.favourited)
        .await?;
    Ok(Json(json!({})))
}

async fn get_source_filters(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let filter_list = state.get_filter_list(id).await?;
    Ok(Json(filter_list))
}

async fn get_pref_schema(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(cached) = state.cache.get_preference_schema(source_id) {
        return Ok(Json(cached));
    }
    let mgr = { state.sources.read().await.get(&source_id).cloned() };
    let raw = if let Some(mgr) = mgr {
        let mut inst = mgr.lease_instance().await.map_err(AppError::CoreError)?;
        inst.get_preferences().await.map_err(AppError::CoreError)?
    } else {
        let name = sqlx::query_scalar!("SELECT name FROM sources WHERE id=?", source_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Source not found".into()))?;
        let wasm_path = state
            .settings
            .read()
            .await
            .wasm_storage_path
            .join(format!("{}.wasm", name));
        let bytes = tokio::fs::read(&wasm_path).await?;
        let component = state
            .wasm_runtime
            .compile_component(&bytes)
            .map_err(AppError::CoreError)?;
        let mut inst =
            kani_core::sources::SourceInstance::new(state.smart_client.clone(), None, false);
        inst.load(
            state.wasm_runtime.engine(),
            &component,
            state.wasm_runtime.linker(),
        )
        .await
        .map_err(AppError::CoreError)?;
        inst.get_preferences().await.map_err(AppError::CoreError)?
    };
    state.cache.insert_preference_schema(source_id, raw.clone());
    Ok(Json(raw))
}

async fn get_source_preferences(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state
        .get_all_preferences(source_id)
        .await?
        .into_iter()
        .map(|(k, v)| json!({ "key": k, "value": v }))
        .collect::<Vec<_>>();
    Ok(Json(rows))
}

async fn set_source_preference(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<SetPreferenceRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.set_preference(source_id, &key, &body.value).await?;
    Ok(Json(json!({})))
}

async fn append_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .append_pref_list_item(source_id, &key, body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn remove_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .remove_pref_list_item(source_id, &key, &body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn toggle_pref_select_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ToggleSelectRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_pref_select_item(source_id, &key, body.item, body.selected)
        .await?;
    Ok(Json(json!({})))
}
