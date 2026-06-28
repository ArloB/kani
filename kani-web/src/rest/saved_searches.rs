use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/saved-searches",
            get(list_saved_searches).post(create_saved_search),
        )
        .route(
            "/saved-searches/{id}",
            put(update_saved_search).delete(delete_saved_search),
        )
}

#[derive(serde::Deserialize)]
pub(crate) struct SavedSearchBody {
    pub name: String,
    pub query_json: String,
}

pub(crate) async fn list_saved_searches(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.service.list_saved_searches(user.id).await?))
}

pub(crate) async fn create_saved_search(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Json(body): Json<SavedSearchBody>,
) -> Result<impl IntoResponse, AppError> {
    let item = state
        .service
        .create_saved_search(user.id, body.name, body.query_json)
        .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub(crate) async fn update_saved_search(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SavedSearchBody>,
) -> Result<impl IntoResponse, AppError> {
    let item = state
        .service
        .update_saved_search(id, user.id, body.name, body.query_json)
        .await?;
    Ok(Json(item))
}

pub(crate) async fn delete_saved_search(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.service.delete_saved_search(id, user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}
