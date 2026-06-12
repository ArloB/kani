//! Chapter download lifecycle & history routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloads/history", get(get_download_history))
        .route("/chapter/{id}/download", post(start_download))
        .route("/chapter/{id}/delete", delete(delete_downloaded))
        .route("/chapter/{id}/cancel", post(cancel_download))
        .route("/downloads/active", delete(cancel_all_global_downloads))
}

async fn get_download_history(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn DownloadDomain>>,
    Query(q): Query<DownloadHistoryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let items = svc.get_download_history(q.limit).await?;
    Ok(Json(items))
}

async fn start_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(svc): State<Arc<dyn DownloadDomain>>,
    Path(id): Path<ChapterId>,
) -> Result<impl IntoResponse, AppError> {
    svc.download_chapter(id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn delete_downloaded(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDelete>,
    State(svc): State<Arc<dyn DownloadDomain>>,
    Path(id): Path<ChapterId>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_downloaded(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn cancel_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(svc): State<Arc<dyn DownloadDomain>>,
    Path(chapter_id): Path<ChapterId>,
) -> Result<impl IntoResponse, AppError> {
    svc.cancel_download(chapter_id).await?;
    Ok(Json(json!({})))
}

async fn cancel_all_global_downloads(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(svc): State<Arc<dyn DownloadDomain>>,
) -> Result<impl IntoResponse, AppError> {
    svc.cancel_all_global_downloads().await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use axum::extract::{Query, State};
    use kani_app::ids::{ChapterId, UserId};
    use kani_app::service::traits::DownloadDomain;
    use std::sync::Arc;

    struct StubDownloads;

    #[async_trait::async_trait]
    impl DownloadDomain for StubDownloads {
        async fn get_download_history(
            &self,
            _limit: i64,
        ) -> kani_app::error::Result<Vec<serde_json::Value>> {
            Ok(vec![serde_json::json!({ "chapter_id": 1 })])
        }
        async fn download_chapter(&self, _: ChapterId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn delete_downloaded(&self, _: ChapterId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn cancel_download(&self, _: ChapterId) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn cancel_all_global_downloads(&self) -> kani_app::error::Result<()> {
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
    async fn download_history_returns_items_without_appservice() {
        let svc: Arc<dyn DownloadDomain> = Arc::new(StubDownloads);
        let response = get_download_history(
            AuthGuard(stub_user(), PhantomData),
            State(svc),
            Query(DownloadHistoryQuery { limit: 10 }),
        )
        .await
        .unwrap();
        let body = axum::response::IntoResponse::into_response(response);
        assert_eq!(body.status(), axum::http::StatusCode::OK);
    }
}
