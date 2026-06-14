//! Scanlator preference & per-manga scanlator/language routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/manga/{id}/scanlator_preferences",
            get(get_scanlator_prefs).post(set_scanlator_pref),
        )
        .route("/scanlator_preferences/{id}", delete(delete_scanlator_pref))
        .route(
            "/manga/{id}/scanlator_mode",
            patch(set_scanlator_mode_handler),
        )
        .route("/manga/{id}/scanlators", get(get_chapter_scanlators))
        .route("/manga/{id}/languages", get(get_chapter_languages))
}

#[utoipa::path(
    get, path = "/rest/manga/{id}/scanlator_preferences",
    params(("id" = i64, Path, description = "Manga ID")),
    responses(
        (status = 200, description = "Per-scanlator priority/block preferences for this manga"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn get_scanlator_prefs(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_scanlator_prefs(manga_id).await?))
}

#[utoipa::path(
    post, path = "/rest/manga/{id}/scanlator_preferences",
    params(("id" = i64, Path, description = "Manga ID")),
    request_body = SetScanlatorPrefRequest,
    responses(
        (status = 200, description = "Scanlator preference saved"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn set_scanlator_pref(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<SetScanlatorPrefRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.set_scanlator_pref(manga_id, &body.scanlator, body.priority, body.blocked)
        .await?;
    Ok(Json(json!({})))
}

#[utoipa::path(
    delete, path = "/rest/scanlator_preferences/{id}",
    params(("id" = i64, Path, description = "Scanlator preference ID")),
    responses(
        (status = 200, description = "Preference deleted"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn delete_scanlator_pref(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(pref_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    svc.delete_scanlator_pref(pref_id).await?;
    Ok(Json(json!({})))
}

#[utoipa::path(
    patch, path = "/rest/manga/{id}/scanlator_mode",
    params(("id" = i64, Path, description = "Manga ID")),
    request_body = SetScanlatorModeRequest,
    responses(
        (status = 200, description = "Scanlator mode updated"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn set_scanlator_mode_handler(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(manga_id): Path<MangaId>,
    Json(body): Json<crate::models::SetScanlatorModeRequest>,
) -> Result<impl IntoResponse, AppError> {
    svc.set_scanlator_mode(manga_id, &body.mode).await?;
    Ok(Json(json!({})))
}

#[utoipa::path(
    get, path = "/rest/manga/{id}/scanlators",
    params(("id" = i64, Path, description = "Manga ID")),
    responses(
        (status = 200, description = "Distinct scanlator names for this manga's chapters"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn get_chapter_scanlators(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_chapter_scanlators(manga_id).await?))
}

#[utoipa::path(
    get, path = "/rest/manga/{id}/languages",
    params(("id" = i64, Path, description = "Manga ID")),
    responses(
        (status = 200, description = "Distinct language codes for this manga's chapters"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "manga"
)]
pub(crate) async fn get_chapter_languages(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(svc): State<Arc<dyn ScanlatorDomain>>,
    Path(manga_id): Path<MangaId>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(svc.get_chapter_languages(manga_id).await?))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use kani_shared::types::ScanlatorPreference;

    fn stub_user() -> crate::auth::User {
        crate::auth::User {
            id: UserId(1),
            username: "stub".into(),
            email: "stub@test.com".into(),
            is_active: true,
            roles: vec![],
            password_hash: String::new(),
            change_id: vec![],
        }
    }

    struct StubScanlators;

    #[async_trait::async_trait]
    impl ScanlatorDomain for StubScanlators {
        async fn get_scanlator_prefs(
            &self,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<Vec<ScanlatorPreference>> {
            Ok(vec![])
        }
        async fn set_scanlator_pref(
            &self,
            _manga_id: MangaId,
            _scanlator: &str,
            _priority: i64,
            _blocked: bool,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn delete_scanlator_pref(&self, _id: i64) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn set_scanlator_mode(
            &self,
            _manga_id: MangaId,
            _mode: &str,
        ) -> kani_app::error::Result<()> {
            unimplemented!()
        }
        async fn get_chapter_scanlators(
            &self,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<Vec<String>> {
            unimplemented!()
        }
        async fn get_chapter_languages(
            &self,
            _manga_id: MangaId,
        ) -> kani_app::error::Result<Vec<String>> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn get_scanlator_prefs_returns_ok_without_appservice() {
        let svc: Arc<dyn ScanlatorDomain> = Arc::new(StubScanlators);
        let response = get_scanlator_prefs(
            AuthGuard(stub_user(), PhantomData),
            State(svc),
            Path(MangaId(1)),
        )
        .await
        .unwrap();
        let resp = axum::response::IntoResponse::into_response(response);
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }
}
