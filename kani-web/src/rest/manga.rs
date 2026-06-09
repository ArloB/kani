//! Per-manga operations, metadata, covers & download-rule routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/manga/{id}", get(get_manga).delete(delete_manga))
        .route(
            "/manga/{id}/cover",
            post(upload_manga_cover_handler).delete(clear_manga_cover_handler),
        )
        .route("/manga/{id}/details", get(get_local_manga_details))
        .route("/manga/{id}/chapters", get(get_local_chapters))
        .route("/manga/{id}/chapter_ids", get(get_chapter_ids))
        .route("/manga/{id}/download_all", post(download_all))
        .route("/manga/{id}/cancel_all", post(cancel_all_downloads))
        .route("/manga/{id}/refresh", post(refresh_manga))
        .route("/manga/{id}/scan", post(scan_manga))
        .route(
            "/manga/{id}/toggle_auto_download",
            post(toggle_auto_download),
        )
        .route("/manga/{id}/toggle_auto_scan", post(toggle_auto_scan_manga))
        .route(
            "/manga/{id}/toggle_download_all_preferred",
            post(toggle_download_all_preferred),
        )
        .route("/manga/{id}/notes", patch(update_manga_notes))
        .route(
            "/manga/{id}/local_metadata",
            patch(update_local_metadata_handler),
        )
        .route("/manga/{id}/seen", patch(mark_manga_seen))
        .route("/manga/{id}/preview_migration", post(preview_migration))
        .route("/manga/{id}/migrate", post(migrate_manga_handler))
        .route(
            "/manga/{id}/download_rules",
            get(get_download_rules).post(add_download_rule),
        )
        .route(
            "/download_rules/{id}",
            delete(delete_download_rule).patch(update_download_rule),
        )
        .route(
            "/manga/{id}/download_rules/order",
            put(reorder_download_rules),
        )
        .route(
            "/manga/{id}/download_rules/preview",
            post(preview_download_rules),
        )
}

async fn get_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_manga_by_id(id).await?))
}

async fn delete_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_manga(id, user.id).await?;
    Ok(Json(json!({})))
}

async fn upload_manga_cover_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::Other("No file field provided".into()))?;
    let content_type = field.content_type().unwrap_or("image/jpeg").to_string();
    const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;
    let bytes = field.bytes().await?;
    if bytes.len() > MAX_COVER_BYTES {
        return Err(AppError::Other("Cover image exceeds 10 MB limit".into()));
    }
    state
        .upload_manga_cover(manga_id, bytes.to_vec(), &content_type, user.id)
        .await?;
    Ok(Json(json!({})))
}

async fn clear_manga_cover_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.clear_manga_cover_override(manga_id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_local_manga_details(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::types::{MangaInfo, MangaStatus};
    let d = state.get_local_manga_details(id).await?;
    let cover_url = if d.manga.local_cover_path.is_some() {
        Some(format!("/rest/manga/{}/cover", id))
    } else {
        d.manga
            .cover_url
            .map(|url| sign_image_url(&url, &d.source.base_url, &state, None))
    };
    let display_name = d
        .manga
        .local_name
        .as_deref()
        .unwrap_or(&d.manga.name)
        .to_owned();
    let display_description = d
        .manga
        .local_description
        .as_ref()
        .or(d.manga.description.as_ref())
        .cloned();
    let display_description_html = display_description
        .as_ref()
        .map(|s| crate::utils::render_description(s));
    let display_status = MangaStatus::from(
        d.manga
            .local_status
            .unwrap_or_else(|| i64::from(d.manga.status)),
    );
    let info = MangaInfo {
        id: d.manga.source_manga_id,
        title: display_name,
        cover_url,
        description: display_description,
        description_html: display_description_html,
        status: display_status,
        authors: d.authors,
        artists: d.artists,
        tags: d.tags,
    };
    Ok(Json(json!({
        "info":                        info,
        "source":                      d.source,
        "auto_download":               d.manga.auto_download,
        "auto_scan":                   d.auto_scan,
        "scanlator_mode":              d.manga.scanlator_mode,
        "download_all_preferred_only": d.manga.download_all_preferred_only,
        "notes":                       d.manga.notes,
        "cover_overridden":            d.manga.cover_overridden,
        "local_name":                  d.manga.local_name,
        "local_description":           d.manga.local_description,
        "local_status":                d.manga.local_status,
        "local_authors":               d.local_authors,
        "local_artists":               d.local_artists,
        "local_tags":                  d.local_tags,
        "has_local_people":            d.has_local_people,
        "has_local_tags":              d.has_local_tags,
        "source_name":                 d.manga.name,
        "source_description":          d.manga.description,
        "source_status":               i64::from(d.manga.status),
        "source_authors":              d.source_authors,
        "source_artists":              d.source_artists,
        "source_tags":                 d.source_tags,
    })))
}

async fn get_local_chapters(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    ValidatedQuery(q): ValidatedQuery<LocalChaptersQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (chapters, has_next_page, total_pages) = state
        .get_local_chapters(
            manga_id,
            q.page,
            q.page_size,
            q.sort_order,
            user.id,
            q.filter_downloaded,
            q.filter_unread,
            q.filter_scanlator,
        )
        .await?;
    Ok(Json(crate::types::ChapterList {
        chapters,
        has_next_page,
        total_pages,
    }))
}

async fn get_chapter_ids(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    ValidatedQuery(q): ValidatedQuery<crate::models::ChapterIdsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let ids = state
        .get_chapter_ids(
            manga_id,
            user.id,
            q.sort_order,
            q.filter_downloaded,
            q.filter_unread,
            q.filter_scanlator,
            q.preferred_only,
        )
        .await?;
    Ok(Json(json!({ "ids": ids })))
}

async fn download_all(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = state_clone.download_all_chapters(manga_id).await {
            tracing::error!(
                "Failed to queue all downloads for manga {}: {}",
                manga_id,
                e
            );
        }
    });
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn cancel_all_downloads(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.cancel_all_downloads(manga_id).await?;
    Ok(Json(json!({})))
}

async fn refresh_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    body: Option<Json<crate::models::RefreshMangaRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let opts = map_refresh_request(req)?;
    state.refresh_manga_with_options(id, opts).await?;
    Ok(Json(json!({})))
}

async fn scan_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let new_chapters = state.scan_for_new_chapters(id).await?.len() as i64;
    Ok(Json(json!({ "new_chapters": new_chapters })))
}

async fn toggle_auto_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.toggle_auto_download(manga_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_auto_scan_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.toggle_auto_scan_manga(manga_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_download_all_preferred(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_download_all_preferred(manga_id, body.enabled)
        .await?;
    Ok(Json(json!({})))
}

async fn update_manga_notes(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let notes = body
        .get("notes")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    state.update_manga_notes(manga_id, notes).await?;
    Ok(Json(json!({})))
}

async fn update_local_metadata_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<crate::models::UpdateLocalMetadataRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .update_local_metadata(
            manga_id,
            kani_app::models::LocalMetadataUpdate {
                local_name: body.local_name,
                local_description: body.local_description,
                local_status: body.local_status,
                authors: body.authors,
                artists: body.artists,
                tags: body.tags,
            },
            user.id,
        )
        .await?;
    Ok(Json(json!({})))
}

async fn mark_manga_seen(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.mark_manga_seen(user.id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn preview_migration(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<PreviewMigrationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let preview = state
        .preview_migration(manga_id, body.target_source_id, body.target_source_manga_id)
        .await?;
    Ok(Json(preview))
}

async fn migrate_manga_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<MigrateMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .migrate_manga(
            manga_id,
            body.target_source_id,
            body.target_source_manga_id,
            body.keep_orphaned_downloads,
        )
        .await?;
    Ok(Json(result))
}

async fn get_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_download_rules(manga_id).await?))
}

async fn add_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<AddDownloadRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.add_download_rule(manga_id, body.kind.clone()).await?;
    Ok((
        StatusCode::CREATED,
        Json(kani_shared::types::DownloadRule {
            id,
            manga_id,
            kind: body.kind,
        }),
    ))
}

async fn delete_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(rule_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_download_rule(rule_id).await?;
    Ok(Json(json!({})))
}

async fn update_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(rule_id): Path<i64>,
    Json(body): Json<UpdateDownloadRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.update_download_rule(rule_id, body.kind).await?;
    Ok(Json(json!({})))
}

async fn reorder_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ReorderDownloadRulesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .reorder_download_rules(manga_id, body.ordered_ids)
        .await?;
    Ok(Json(json!({})))
}

async fn preview_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<PreviewDownloadRulesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (matching, total) = state.preview_download_rules(manga_id, body.kinds).await?;
    Ok(Json(json!({ "matching": matching, "total": total })))
}
