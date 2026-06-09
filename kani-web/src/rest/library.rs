//! Library listing, backup/restore, import & duplicate routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/library", get(get_library_filtered))
        .route("/library/scan-all", post(scan_all_library))
        .route("/manga/scan", post(scan_manga_multiple))
        .route("/library/continue_reading", get(get_continue_reading_shelf))
        .route("/library/{page}/{order}", get(get_library))
        .route("/library/backup", get(library_backup))
        .route(
            "/library/backup/preview",
            post(library_backup_preview).route_layer(DefaultBodyLimit::max(MAX_BACKUP_BYTES)),
        )
        .route(
            "/library/restore",
            post(library_restore).route_layer(DefaultBodyLimit::max(MAX_BACKUP_BYTES)),
        )
        .route(
            "/library/import/tachiyomi/preview",
            post(library_tachiyomi_preview).route_layer(DefaultBodyLimit::max(MAX_TACHI_BYTES)),
        )
        .route(
            "/library/import/tachiyomi",
            post(library_import_tachiyomi).route_layer(DefaultBodyLimit::max(MAX_TACHI_BYTES)),
        )
        .route("/library/pending-imports", get(library_pending_imports))
        .route(
            "/library/pending-imports/{id}",
            delete(library_delete_pending_import),
        )
        .route(
            "/library/pending-imports/{id}/resolve",
            post(library_resolve_pending_import),
        )
        .route("/library/orphaned", get(library_orphaned))
        .route("/library/duplicates", get(library_duplicates))
        .route("/library/duplicates/merge", post(library_merge_duplicate))
        .route("/library/duplicates/scan", post(library_duplicates_scan))
        .route(
            "/library/duplicates/{a}/{b}/dismiss",
            post(library_dismiss_duplicate),
        )
        .route("/recent_updates", get(get_recent_updates))
        .route("/global_search", get(global_search_handler))
}

async fn get_library_filtered(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<LibraryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (records, has_next_page, total_pages) = state
        .get_library_filtered(
            user.id,
            q.page,
            q.page_size,
            q.search,
            q.status_filter,
            q.tag_filter,
            q.author_filter,
            q.artist_filter,
            q.category_filter,
            q.reading_status_filter,
            q.hide_no_unread,
            q.hide_completed_status,
            q.source_id,
            q.sort_by,
        )
        .await?;

    let items = records
        .into_iter()
        .map(|r| {
            let cover_url = if r.local_cover_path.is_some() {
                Some(format!("/rest/manga/{}/cover", r.id))
            } else {
                r.cover_url
                    .map(|url| sign_image_url(&url, &r.base_url, &state, None))
            };
            crate::types::MangaListItem {
                id: r.id.to_string(),
                title: r.name,
                cover_url,
                new_chapter_count: r.new_chapter_count,
            }
        })
        .collect();

    Ok(Json(crate::types::LibraryPage {
        items,
        has_next_page,
        total_pages,
    }))
}

async fn scan_all_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let queued = state.scan_all_manga().await?;
    Ok(Json(json!({ "queued": queued })))
}

/// Unified scan endpoint: scan all library manga or a specific list of IDs.
/// Both paths emit `Started` / `MangaRefreshed` / `Completed` SSE events,
/// so the frontend can use identical progress handling for both cases.
///
/// Body: `{ "ids": "all" }` or `{ "ids": [1, 2, 3] }`
async fn scan_manga_multiple(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
    Json(body): Json<ScanMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    match body {
        ScanMangaRequest::All { .. } => {
            state.scan_all_manga().await?;
        }
        ScanMangaRequest::Ids { ids } => {
            state.scan_manga_ids(ids).await?;
        }
    }
    Ok(StatusCode::ACCEPTED)
}

async fn get_continue_reading_shelf(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Query(q): Query<ContinueReadingShelfQuery>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.get_continue_reading_shelf(user.id, q.limit).await?;
    let response: Vec<_> = items
        .into_iter()
        .map(|item| {
            let cover_url = if item.local_cover_path.is_some() {
                Some(format!("/rest/manga/{}/cover", item.manga_id))
            } else {
                item.cover_url
                    .map(|url| sign_image_url(&url, &item.base_url, &state, None))
            };
            json!({
                "manga_id": item.manga_id,
                "manga_name": item.manga_name,
                "cover_url": cover_url,
                "chapter_id": item.chapter_id,
                "chapter_number": item.chapter_number,
                "last_page": item.last_page,
            })
        })
        .collect();
    Ok(Json(response))
}

async fn get_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path((page, order)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_library(page, order).await?))
}

async fn library_backup(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Query(q): Query<LibraryBackupQuery>,
) -> Result<impl IntoResponse, AppError> {
    let include_progress = q.include_chapter_progress.unwrap_or(false);
    let bytes = state.export_backup(user.id, include_progress).await?;

    let now = time::OffsetDateTime::now_utc();
    let filename = format!(
        "kani-backup-{}-{:02}-{:02}.zip",
        now.year(),
        now.month() as u8,
        now.day()
    );
    let disposition = format!("attachment; filename=\"{filename}\"");

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(Body::from(bytes))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

async fn library_backup_preview(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let bytes = collect_file_field(&mut multipart, MAX_BACKUP_BYTES).await?;
    let preview = state.preview_backup(&bytes).await?;
    Ok(Json(preview))
}

async fn library_restore(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut opts = kani_app::RestoreOptions::default();

    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("file") => {
                let content_length = field
                    .headers()
                    .get(rquest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<usize>().ok());
                file_bytes = Some(
                    kani_core::http::collect_bytes_limited(
                        Box::pin(field.map_err(|e| kani_core::error::Error::Other(e.to_string()))),
                        content_length,
                        MAX_BACKUP_BYTES,
                    )
                    .await?,
                );
            }
            Some("merge") => {
                opts.merge = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
            }
            Some("import_manga") => {
                opts.import_manga = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_categories") => {
                opts.import_categories = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_download_rules") => {
                opts.import_download_rules = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_tracking") => {
                opts.import_tracking = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_chapter_progress") => {
                opts.import_chapter_progress = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
            }
            Some("import_settings") => {
                opts.import_settings = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
            }
            _ => {}
        }
    }

    let data = file_bytes.ok_or_else(|| AppError::ValidationError("No file uploaded".into()))?;
    let result = state.restore_backup(user.id, &data, opts).await?;
    state
        .audit(
            Some(user.id),
            "backup.restore",
            None,
            Some(json!({ "imported_manga": result.imported_manga })),
        )
        .await;
    Ok(Json(result))
}

async fn library_tachiyomi_preview(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let bytes = collect_file_field(&mut multipart, MAX_TACHI_BYTES).await?;
    let preview = state.preview_tachiyomi_backup(&bytes).await?;
    Ok(Json(preview))
}

async fn library_import_tachiyomi(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut file_bytes: Option<bytes::Bytes> = None;
    let mut opts = kani_app::TachiyomiImportOptions::default();

    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("file") => {
                let content_length = field
                    .headers()
                    .get(rquest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<usize>().ok());
                file_bytes = Some(
                    kani_core::http::collect_bytes_limited(
                        Box::pin(field.map_err(|e| kani_core::error::Error::Other(e.to_string()))),
                        content_length,
                        MAX_TACHI_BYTES,
                    )
                    .await?,
                );
            }
            Some("import_manga") => {
                opts.import_manga = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_categories") => {
                opts.import_categories = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_tracking") => {
                opts.import_tracking = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(true);
            }
            Some("import_chapter_progress") => {
                opts.import_chapter_progress = field
                    .text()
                    .await
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(false);
            }
            _ => {}
        }
    }

    let data = file_bytes.ok_or_else(|| AppError::ValidationError("No file uploaded".into()))?;
    let result = state.import_tachiyomi_backup(user.id, &data, opts).await?;
    Ok(Json(result))
}

async fn library_pending_imports(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.list_pending_imports(user.id).await?;
    Ok(Json(items))
}

async fn library_delete_pending_import(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_pending_import(user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn library_resolve_pending_import(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ResolvePendingImportBody>,
) -> Result<impl IntoResponse, AppError> {
    let manga_id = state
        .resolve_pending_import(user.id, id, body.source_id, &body.source_manga_id)
        .await?;
    Ok(Json(json!({ "manga_id": manga_id })))
}

async fn library_orphaned(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.list_orphaned_manga().await?;
    Ok(Json(items))
}

async fn library_duplicates(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let pairs = kani_app::service::dedup::list_duplicate_pairs(&state.db).await?;
    Ok(Json(pairs))
}

async fn library_merge_duplicate(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<MergeDuplicateBody>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::dedup::merge_manga(&state.db, body.keep_id, body.discard_id).await?;
    state
        .audit(
            Some(user.id),
            "manga.merge_duplicate",
            None,
            Some(serde_json::json!({ "keep": body.keep_id, "discard": body.discard_id })),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn library_duplicates_scan(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let new_pairs = kani_app::service::dedup::scan_and_persist_duplicates(&state.db).await?;
    state
        .audit(
            Some(user.id),
            "library.duplicates_scan",
            None,
            Some(serde_json::json!({ "new_pairs": new_pairs })),
        )
        .await;
    Ok(Json(serde_json::json!({ "new_pairs": new_pairs })))
}

async fn library_dismiss_duplicate(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    axum::extract::Path(path): axum::extract::Path<DismissDuplicatePath>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::dedup::dismiss_duplicate_pair(&state.db, path.a, path.b).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_recent_updates(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<PageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (mut items, has_next_page, total_pages) = state.get_recent_updates(q.page).await?;
    for u in &mut items {
        u.cover_url = if u.local_cover_path.is_some() {
            Some(format!("/rest/manga/{}/cover", u.manga_id))
        } else if let Some(ref url) = u.cover_url.clone() {
            Some(sign_image_url(url, &u.base_url, &state, None))
        } else {
            None
        };
    }
    Ok(Json(crate::types::RecentUpdate {
        recent_updates: items,
        has_next_page,
        total_pages,
    }))
}

async fn global_search_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Query(q): Query<GlobalSearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let mut list = state
        .global_search(&q.query, q.scope, q.page, q.page_size)
        .await?;

    let source_ids: Vec<i64> = list
        .iter()
        .map(|i| i.source_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let source_ids_json = serde_json::to_string(&source_ids)?;
    let base_urls: std::collections::HashMap<i64, String> = sqlx::query!(
        "SELECT id, base_url FROM sources WHERE id IN (SELECT value FROM json_each(?))",
        source_ids_json
    )
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|r| (r.id, r.base_url))
    .collect();

    for result in &mut list {
        let referer = base_urls
            .get(&result.source_id)
            .map(String::as_str)
            .unwrap_or("");
        for item in &mut result.manga {
            if let Some(ref url) = item.cover_url.clone() {
                item.cover_url = Some(sign_image_url(url, referer, &state, None));
            }
        }
    }
    Ok(Json(list))
}
