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

// cross-domain: sign_image_url requires proxy_secret from AppState
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
    State(svc): State<Arc<dyn LibraryDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let queued = svc.scan_all_manga().await?;
    Ok(Json(json!({ "queued": queued })))
}

/// Unified scan endpoint: scan all library manga or a specific list of IDs.
/// Both paths emit `Started` / `MangaRefreshed` / `Completed` SSE events,
/// so the frontend can use identical progress handling for both cases.
///
/// Body: `{ "ids": "all" }` or `{ "ids": [1, 2, 3] }`
async fn scan_manga_multiple(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    Json(body): Json<ScanMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    match body {
        ScanMangaRequest::All { .. } => {
            svc.scan_all_manga().await?;
        }
        ScanMangaRequest::Ids { ids } => {
            svc.scan_manga_ids(ids).await?;
        }
    }
    Ok(StatusCode::ACCEPTED)
}

// cross-domain: sign_image_url requires proxy_secret from AppState
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
    State(svc): State<Arc<dyn LibraryDomain>>,
    Path((page, order)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_library(page, order).await?))
}

async fn library_backup(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    Query(q): Query<LibraryBackupQuery>,
) -> Result<impl IntoResponse, AppError> {
    let include_progress = q.include_chapter_progress.unwrap_or(false);
    let bytes = svc.export_backup(user.id, include_progress).await?;

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
    State(svc): State<Arc<dyn LibraryDomain>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let bytes = collect_file_field(&mut multipart, MAX_BACKUP_BYTES).await?;
    let preview = svc.preview_backup(&bytes).await?;
    Ok(Json(preview))
}

async fn library_restore(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
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
    let result = svc.restore_backup(user.id, &data, opts).await?;
    Ok(Json(result))
}

async fn library_tachiyomi_preview(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let bytes = collect_file_field(&mut multipart, MAX_TACHI_BYTES).await?;
    let preview = svc.preview_tachiyomi_backup(&bytes).await?;
    Ok(Json(preview))
}

async fn library_import_tachiyomi(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
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
    let result = svc.import_tachiyomi_backup(user.id, &data, opts).await?;
    Ok(Json(result))
}

async fn library_pending_imports(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn LibraryDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let items = svc.list_pending_imports(user.id).await?;
    Ok(Json(items))
}

async fn library_delete_pending_import(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_pending_import(user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn library_resolve_pending_import(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    Path(id): Path<i64>,
    Json(body): Json<ResolvePendingImportBody>,
) -> Result<impl IntoResponse, AppError> {
    let manga_id = svc
        .resolve_pending_import(user.id, id, body.source_id, &body.source_manga_id)
        .await?;
    Ok(Json(json!({ "manga_id": manga_id })))
}

async fn library_orphaned(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn LibraryDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let items = svc.list_orphaned_manga().await?;
    Ok(Json(items))
}

async fn library_duplicates(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let pairs = svc.list_duplicates().await?;
    Ok(Json(pairs))
}

async fn library_merge_duplicate(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    Json(body): Json<MergeDuplicateBody>,
) -> Result<impl IntoResponse, AppError> {
    svc.merge_duplicate(body.keep_id, body.discard_id, user.id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn library_duplicates_scan(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
) -> Result<impl IntoResponse, AppError> {
    let new_pairs = svc.scan_duplicates(user.id).await?;
    Ok(Json(serde_json::json!({ "new_pairs": new_pairs })))
}

async fn library_dismiss_duplicate(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn LibraryDomain>>,
    axum::extract::Path(path): axum::extract::Path<DismissDuplicatePath>,
) -> Result<impl IntoResponse, AppError> {
    svc.dismiss_duplicate(path.a, path.b).await?;
    Ok(StatusCode::NO_CONTENT)
}

// cross-domain: sign_image_url requires proxy_secret from AppState
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

// cross-domain: sign_image_url requires proxy_secret from AppState; direct state.db SQL query
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::extract::State;
    use kani_app::ids::{MangaId, UserId};
    use kani_app::models::Manga;
    use kani_app::service::dedup::DuplicatePair;
    use kani_app::service::traits::LibraryDomain;
    use kani_app::{
        BackupPreview, RestoreOptions, RestoreResult, TachiyomiImportOptions,
        TachiyomiImportResult, TachiyomiPreview,
    };
    use kani_app::{OrphanedManga, models::PendingImportRow};
    use std::sync::Arc;

    struct StubLibrary;

    #[async_trait::async_trait]
    impl LibraryDomain for StubLibrary {
        async fn list_orphaned_manga(&self) -> kani_app::error::Result<Vec<OrphanedManga>> {
            Ok(vec![OrphanedManga {
                id: 7,
                name: "Gone Source Manga".into(),
                cover_url: None,
                local_cover_path: None,
                source_name: "removed-ext".into(),
            }])
        }
        async fn scan_all_manga(&self) -> kani_app::error::Result<usize> {
            unimplemented!()
        }
        async fn scan_manga_ids(&self, _: Vec<MangaId>) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn get_library(&self, _: i32, _: i32) -> kani_app::error::Result<Vec<Manga>> {
            unimplemented!()
        }
        async fn export_backup(&self, _: UserId, _: bool) -> kani_app::error::Result<Vec<u8>> {
            unimplemented!()
        }
        async fn preview_backup(&self, _: &[u8]) -> kani_app::error::Result<BackupPreview> {
            unimplemented!()
        }
        async fn restore_backup(
            &self,
            _: UserId,
            _: &[u8],
            _: RestoreOptions,
        ) -> kani_app::error::Result<RestoreResult> {
            unimplemented!()
        }
        async fn preview_tachiyomi_backup(
            &self,
            _: &[u8],
        ) -> kani_app::error::Result<TachiyomiPreview> {
            unimplemented!()
        }
        async fn import_tachiyomi_backup(
            &self,
            _: UserId,
            _: &[u8],
            _: TachiyomiImportOptions,
        ) -> kani_app::error::Result<TachiyomiImportResult> {
            unimplemented!()
        }
        async fn list_pending_imports(
            &self,
            _: UserId,
        ) -> kani_app::error::Result<Vec<PendingImportRow>> {
            unimplemented!()
        }
        async fn delete_pending_import(&self, _: UserId, _: i64) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn resolve_pending_import(
            &self,
            _: UserId,
            _: i64,
            _: i64,
            _: &str,
        ) -> kani_app::error::Result<MangaId> {
            unimplemented!()
        }
        async fn list_duplicates(&self) -> kani_app::error::Result<Vec<DuplicatePair>> {
            unimplemented!()
        }
        async fn merge_duplicate(&self, _: i64, _: i64, _: UserId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn scan_duplicates(&self, _: UserId) -> kani_app::error::Result<u32> {
            unimplemented!()
        }
        async fn dismiss_duplicate(&self, _: MangaId, _: MangaId) -> kani_app::error::Result<()> {
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
    async fn library_orphaned_returns_items_without_appservice() {
        let svc: Arc<dyn LibraryDomain> = Arc::new(StubLibrary);
        let response = library_orphaned(AuthGuard(stub_user(), PhantomData), State(svc))
            .await
            .unwrap();
        let body = axum::response::IntoResponse::into_response(response);
        assert_eq!(body.status(), axum::http::StatusCode::OK);
    }
}
