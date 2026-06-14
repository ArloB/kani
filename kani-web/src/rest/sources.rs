//! Extension source management & browsing routes.

use super::*;
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sources", get(list_sources).post(add_source))
        .route("/sources/health", get(get_sources_health))
        .route("/sources/active_ids", get(get_active_source_ids))
        .route("/sources/metadata-providers", get(list_metadata_providers))
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
        .route("/sources/{id}/capabilities", get(get_capabilities))
}

async fn list_sources(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.list_sources().await?))
}

async fn add_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(svc): State<Arc<dyn SourceDomain>>,
    ValidatedJson(payload): ValidatedJson<CreateSource>,
) -> Result<impl IntoResponse, AppError> {
    let id = svc.add_source(&payload.name, user.id).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn get_sources_health(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_source_health().await?))
}

async fn get_active_source_ids(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let ids = svc.list_active_source_ids().await?;
    Ok(Json(ids))
}

async fn get_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let source = svc.get_source(id).await?;
    Ok(Json(source))
}

async fn update_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(id): Path<i64>,
    ValidatedJson(payload): ValidatedJson<UpdateSource>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_none() && payload.version.is_none() {
        return Ok(Json(json!({})));
    }
    svc.update_source(id, payload.name, payload.version).await?;
    Ok(Json(json!({})))
}

async fn delete_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceDelete>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_source(id, user.id).await?;
    Ok(Json(json!({})))
}

async fn get_metadata(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = svc.get_metadata(id).await?;
    Ok(result)
}

// cross-domain: needs proxy_client + install_source
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

// cross-domain: needs proxy_client
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

// cross-domain: needs wasm_runtime + sources field
async fn reload_source_handler(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    reload_source(&state, id).await?;
    Ok(StatusCode::OK)
}

// cross-domain: needs proxy_secret for sign_image_url
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

// cross-domain: needs proxy_secret for sign_image_url
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

// cross-domain: needs proxy_secret for sign_image_url
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
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let url = svc.get_source_url(id, &manga_id).await?;
    Ok(Json(serde_json::json!({ "url": url })))
}

// cross-domain: library domain
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
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((id, manga_id, page, page_size)): Path<(i64, String, i32, i32)>,
    Query(q): Query<ChapterListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let json_str = svc
        .get_chapter_list_paged(id, &manga_id, page, page_size, q.sort)
        .await?;
    let list: crate::types::ChapterList = serde_json::from_str(&json_str)?;
    Ok(Json(list))
}

async fn get_chapter_sort_list(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((id, _manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let opts = svc.get_chapter_sort_list(id).await?;
    Ok(Json(opts))
}

// cross-domain: needs proxy_secret for sign_image_url
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

// cross-domain: library domain
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
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleEnabledRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_source_enabled(source_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_source_favourite(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleFavouritedRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_source_favourite(source_id, body.favourited)
        .await?;
    Ok(Json(json!({})))
}

async fn get_source_filters(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let filter_list = svc.get_filter_list(id).await?;
    Ok(Json(filter_list))
}

async fn get_pref_schema(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let schema = svc.get_source_pref_schema(source_id).await?;
    Ok(Json(schema))
}

async fn get_source_preferences(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = svc
        .get_all_preferences(source_id)
        .await?
        .into_iter()
        .map(|(k, v)| json!({ "key": k, "value": v }))
        .collect::<Vec<_>>();
    Ok(Json(rows))
}

async fn set_source_preference(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<SetPreferenceRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.set_preference(source_id, &key, &body.value).await?;
    Ok(Json(json!({})))
}

async fn append_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.append_pref_list_item(source_id, &key, body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn remove_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.remove_pref_list_item(source_id, &key, &body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn toggle_pref_select_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(svc): State<Arc<dyn SourceDomain>>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ToggleSelectRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_pref_select_item(source_id, &key, body.item, body.selected)
        .await?;
    Ok(Json(json!({})))
}

async fn list_metadata_providers(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let registry = state.metadata_provider_registry.read().await;
    Ok(Json(registry.list()))
}

#[derive(serde::Serialize)]
struct SourceCapabilities {
    streaming_chapters: bool,
}

async fn get_capabilities(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let row =
        sqlx::query("SELECT streaming_chapters FROM sources WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::InternalServerError(e.to_string()))?
            .ok_or_else(|| AppError::NotFound(format!("Source {id} not found")))?;
    let streaming: bool = row.get::<i64, _>(0) != 0;
    Ok(Json(SourceCapabilities {
        streaming_chapters: streaming,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::extract::State;
    use kani_app::ids::UserId;
    use kani_app::models::SourceHealthRow;
    use kani_app::service::traits::SourceDomain;
    use kani_shared::types::{ChapterSortOption, Source};
    use std::sync::Arc;

    struct StubSources;

    #[async_trait::async_trait]
    impl SourceDomain for StubSources {
        async fn list_sources(&self) -> kani_app::error::Result<Vec<Source>> {
            Ok(vec![Source {
                id: 1,
                name: "stub-ext".into(),
                version: "1.0".into(),
                base_url: "https://example.com".into(),
                enabled: true,
                favourited: false,
                unrestricted_http: false,
            }])
        }
        async fn get_source(&self, _: i64) -> kani_app::error::Result<Source> {
            unimplemented!()
        }
        async fn add_source(&self, _: &str, _: UserId) -> kani_app::error::Result<i64> {
            unimplemented!()
        }
        async fn update_source(
            &self,
            _: i64,
            _: Option<String>,
            _: Option<String>,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn get_source_health(&self) -> kani_app::error::Result<Vec<SourceHealthRow>> {
            unimplemented!()
        }
        async fn get_metadata(&self, _: i64) -> kani_app::error::Result<String> {
            unimplemented!()
        }
        async fn toggle_source_enabled(&self, _: i64, _: bool) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn toggle_source_favourite(&self, _: i64, _: bool) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn get_filter_list(
            &self,
            _: i64,
        ) -> kani_app::error::Result<kani_core::WitFilterList> {
            unimplemented!()
        }
        async fn get_all_preferences(
            &self,
            _: i64,
        ) -> kani_app::error::Result<Vec<(String, String)>> {
            unimplemented!()
        }
        async fn set_preference(&self, _: i64, _: &str, _: &str) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn append_pref_list_item(
            &self,
            _: i64,
            _: &str,
            _: String,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn remove_pref_list_item(
            &self,
            _: i64,
            _: &str,
            _: &str,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn toggle_pref_select_item(
            &self,
            _: i64,
            _: &str,
            _: String,
            _: bool,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn get_source_url(&self, _: i64, _: &str) -> kani_app::error::Result<String> {
            unimplemented!()
        }
        async fn get_chapter_list_paged(
            &self,
            _: i64,
            _: &str,
            _: i32,
            _: i32,
            _: Option<String>,
        ) -> kani_app::error::Result<String> {
            unimplemented!()
        }
        async fn get_chapter_sort_list(
            &self,
            _: i64,
        ) -> kani_app::error::Result<Vec<ChapterSortOption>> {
            unimplemented!()
        }
        async fn delete_source(&self, _: i64, _: UserId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn list_active_source_ids(&self) -> kani_app::error::Result<Vec<i64>> {
            unimplemented!()
        }
        async fn get_source_pref_schema(
            &self,
            _: i64,
        ) -> kani_app::error::Result<Vec<kani_core::PreferenceSpec>> {
            unimplemented!()
        }
    }

    fn stub_user() -> crate::auth::User {
        crate::auth::User {
            id: UserId(1),
            username: "test".into(),
            email: "test@example.com".into(),
            is_active: true,
            roles: vec![],
            password_hash: String::new(),
            change_id: vec![],
        }
    }

    #[tokio::test]
    async fn list_sources_returns_items_without_appservice() {
        let svc: Arc<dyn SourceDomain> = Arc::new(StubSources);
        let response = list_sources(AuthGuard(stub_user(), PhantomData), State(svc))
            .await
            .unwrap();
        let body = axum::response::IntoResponse::into_response(response);
        assert_eq!(body.status(), axum::http::StatusCode::OK);
    }
}
