//! Category CRUD & per-manga category routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/categories", get(list_categories).post(create_category))
        .route("/categories/reorder", put(reorder_categories))
        .route(
            "/categories/{id}",
            patch(rename_category).delete(delete_category_handler),
        )
        .route(
            "/manga/{id}/categories",
            get(get_manga_categories).put(set_manga_categories),
        )
}

async fn list_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.list_categories().await?))
}

async fn create_category(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.create_category(&body.name, body.sort_order).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn reorder_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<ReorderCategoriesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.reorder_categories(body.ordered_ids).await?;
    Ok(Json(json!({})))
}

async fn rename_category(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(category_id): Path<i64>,
    Json(body): Json<RenameCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.rename_category(category_id, &body.name).await?;
    Ok(Json(json!({})))
}

async fn delete_category_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(category_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_category(category_id).await?;
    Ok(Json(json!({})))
}

async fn get_manga_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_manga_categories(manga_id).await?))
}

async fn set_manga_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetMangaCategoriesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_manga_categories(manga_id, body.category_ids)
        .await?;
    Ok(Json(json!({})))
}
