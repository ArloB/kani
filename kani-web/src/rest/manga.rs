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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_manga_by_id(id).await?))
}

async fn delete_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryDelete>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_manga(id, user.id).await?;
    Ok(Json(json!({})))
}

async fn upload_manga_cover_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
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
    svc.upload_manga_cover(manga_id, bytes.to_vec(), &content_type, user.id)
        .await?;
    Ok(Json(json!({})))
}

async fn clear_manga_cover_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    svc.clear_manga_cover_override(manga_id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// cross-domain: sign_image_url needs proxy_secret from AppState
async fn get_local_manga_details(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<MangaId>,
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    ValidatedQuery(q): ValidatedQuery<LocalChaptersQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (chapters, has_next_page, total_pages) = svc
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    ValidatedQuery(q): ValidatedQuery<crate::models::ChapterIdsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let ids = svc
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    let svc = svc.clone();
    tokio::spawn(async move {
        if let Err(e) = svc.download_all_chapters(manga_id).await {
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    svc.cancel_all_downloads(manga_id).await?;
    Ok(Json(json!({})))
}

async fn refresh_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(id): Path<MangaId>,
    body: Option<Json<crate::models::RefreshMangaRequest>>,
) -> Result<impl IntoResponse, AppError> {
    let req = body.map(|Json(b)| b).unwrap_or_default();
    let opts = map_refresh_request(req)?;
    svc.refresh_manga_with_options(id, opts).await?;
    Ok(Json(json!({})))
}

async fn scan_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    let new_chapters = svc.scan_for_new_chapters(id).await?.len() as i64;
    Ok(Json(json!({ "new_chapters": new_chapters })))
}

async fn toggle_auto_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_auto_download(manga_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_auto_scan_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_auto_scan_manga(manga_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_download_all_preferred(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.toggle_download_all_preferred(manga_id, body.enabled)
        .await?;
    Ok(Json(json!({})))
}

async fn update_manga_notes(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let notes = body
        .get("notes")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    svc.update_manga_notes(manga_id, notes).await?;
    Ok(Json(json!({})))
}

async fn update_local_metadata_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<crate::models::UpdateLocalMetadataRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.update_local_metadata(
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    svc.mark_manga_seen(user.id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn preview_migration(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<PreviewMigrationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let preview = svc
        .preview_migration(manga_id, body.target_source_id, body.target_source_manga_id)
        .await?;
    Ok(Json(preview))
}

async fn migrate_manga_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<MigrateMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = svc
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
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_download_rules(manga_id).await?))
}

async fn add_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<AddDownloadRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = svc.add_download_rule(manga_id, body.kind.clone()).await?;
    Ok((
        StatusCode::CREATED,
        Json(kani_shared::types::DownloadRule {
            id,
            manga_id: manga_id.0,
            kind: body.kind,
        }),
    ))
}

async fn delete_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(rule_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_download_rule(rule_id).await?;
    Ok(Json(json!({})))
}

async fn update_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(rule_id): Path<i64>,
    Json(body): Json<UpdateDownloadRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.update_download_rule(rule_id, body.kind).await?;
    Ok(Json(json!({})))
}

async fn reorder_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<ReorderDownloadRulesRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.reorder_download_rules(manga_id, body.ordered_ids)
        .await?;
    Ok(Json(json!({})))
}

async fn preview_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn MangaDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<PreviewDownloadRulesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (matching, total) = svc.preview_download_rules(manga_id, body.kinds).await?;
    Ok(Json(json!({ "matching": matching, "total": total })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::extract::{Path, State};
    use kani_app::ids::{MangaId, UserId};
    use kani_app::service::traits::MangaDomain;
    use kani_shared::types::{
        Chapter, ChapterSortOrder, DownloadRule, DownloadRuleKind, MigrationPreview,
        MigrationResult,
    };
    use std::sync::Arc;

    struct StubManga;

    #[async_trait::async_trait]
    impl MangaDomain for StubManga {
        async fn get_download_rules(
            &self,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<Vec<DownloadRule>> {
            Ok(vec![])
        }
        async fn get_manga_by_id(
            &self,
            _id: MangaId,
        ) -> kani_app::error::Result<kani_app::models::Manga> {
            unimplemented!()
        }
        async fn delete_manga(
            &self,
            _id: MangaId,
            _user_id: UserId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn upload_manga_cover(
            &self,
            _manga_id: MangaId,
            _bytes: Vec<u8>,
            _content_type: &str,
            _user_id: UserId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn clear_manga_cover_override(
            &self,
            _manga_id: MangaId,
            _user_id: UserId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        #[allow(clippy::too_many_arguments)]
        async fn get_local_chapters(
            &self,
            _manga_id: MangaId,
            _page: i32,
            _page_size: i32,
            _sort_order: ChapterSortOrder,
            _user_id: UserId,
            _filter_downloaded: Option<bool>,
            _filter_unread: Option<bool>,
            _filter_scanlator: Option<String>,
        ) -> kani_app::error::Result<(Vec<Chapter>, bool, Option<u32>)> {
            unimplemented!()
        }
        #[allow(clippy::too_many_arguments)]
        async fn get_chapter_ids(
            &self,
            _manga_id: MangaId,
            _user_id: UserId,
            _sort_order: ChapterSortOrder,
            _filter_downloaded: Option<bool>,
            _filter_unread: Option<bool>,
            _filter_scanlator: Option<String>,
            _preferred_only: bool,
        ) -> kani_app::error::Result<Vec<kani_app::ids::ChapterId>> {
            unimplemented!()
        }
        async fn download_all_chapters(&self, _manga_id: MangaId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn cancel_all_downloads(&self, _manga_id: MangaId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn refresh_manga_with_options(
            &self,
            _manga_id: MangaId,
            _opts: kani_app::models::RefreshOptions,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn scan_for_new_chapters(
            &self,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<Vec<i64>> {
            unimplemented!()
        }
        async fn toggle_auto_download(
            &self,
            _manga_id: MangaId,
            _enabled: bool,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn toggle_auto_scan_manga(
            &self,
            _manga_id: MangaId,
            _enabled: bool,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn toggle_download_all_preferred(
            &self,
            _manga_id: MangaId,
            _enabled: bool,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn update_manga_notes(
            &self,
            _manga_id: MangaId,
            _notes: Option<String>,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn update_local_metadata(
            &self,
            _manga_id: MangaId,
            _update: kani_app::models::LocalMetadataUpdate,
            _user_id: UserId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn mark_manga_seen(
            &self,
            _user_id: UserId,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn preview_migration(
            &self,
            _manga_id: MangaId,
            _target_source_id: i64,
            _target_source_manga_id: String,
        ) -> kani_app::error::Result<MigrationPreview> {
            unimplemented!()
        }
        async fn migrate_manga(
            &self,
            _manga_id: MangaId,
            _target_source_id: i64,
            _target_source_manga_id: String,
            _keep_orphaned_downloads: bool,
        ) -> kani_app::error::Result<MigrationResult> {
            unimplemented!()
        }
        async fn add_download_rule(
            &self,
            _manga_id: MangaId,
            _kind: DownloadRuleKind,
        ) -> kani_app::error::Result<i64> {
            unimplemented!()
        }
        async fn delete_download_rule(&self, _rule_id: i64) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn update_download_rule(
            &self,
            _rule_id: i64,
            _kind: DownloadRuleKind,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn reorder_download_rules(
            &self,
            _manga_id: MangaId,
            _ordered_ids: Vec<i64>,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn preview_download_rules(
            &self,
            _manga_id: MangaId,
            _kinds: Vec<DownloadRuleKind>,
        ) -> kani_app::error::Result<(usize, usize)> {
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
    async fn download_rules_returns_empty_without_appservice() {
        let svc: Arc<dyn MangaDomain> = Arc::new(StubManga);
        let response = get_download_rules(
            AuthGuard(stub_user(), PhantomData),
            State(svc),
            Path(MangaId(1)),
        )
        .await
        .unwrap();
        let body = axum::response::IntoResponse::into_response(response);
        assert_eq!(body.status(), axum::http::StatusCode::OK);
    }
}
