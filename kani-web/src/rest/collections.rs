use super::*;
use kani_app::service::smart_collections::SmartCollectionRule;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/collections/{id}",
            put(update_collection).delete(delete_collection),
        )
        .route("/collections/{id}/manga", get(get_collection_manga))
}

#[derive(serde::Deserialize)]
pub(crate) struct CollectionBody {
    pub name: String,
    pub rule: SmartCollectionRule,
    #[serde(default)]
    pub sort_order: i64,
}

pub(crate) async fn list_collections(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.service.list_collections().await?))
}

pub(crate) async fn create_collection(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<CollectionBody>,
) -> Result<impl IntoResponse, AppError> {
    let col = state
        .service
        .create_collection(body.name, &body.rule, body.sort_order)
        .await?;
    Ok((StatusCode::CREATED, Json(col)))
}

pub(crate) async fn update_collection(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<CollectionBody>,
) -> Result<impl IntoResponse, AppError> {
    let col = state
        .service
        .update_collection(id, body.name, &body.rule, body.sort_order)
        .await?;
    Ok(Json(col))
}

pub(crate) async fn delete_collection(
    _: AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.service.delete_collection(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn get_collection_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let col = state.service.get_collection(id).await?;
    let rule: SmartCollectionRule = serde_json::from_str(&col.rule_json)
        .map_err(|e| AppError::InternalServerError(format!("Invalid rule JSON: {e}")))?;
    let ids = state.service.evaluate_collection(&rule, user.id).await?;
    Ok(Json(json!({ "manga_ids": ids })))
}
