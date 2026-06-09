//! Library filter facet routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/filters/tags", get(get_filter_tags))
        .route("/filters/authors", get(get_filter_authors))
        .route("/filters/artists", get(get_filter_artists))
}

async fn get_filter_tags(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_tags().await?))
}

async fn get_filter_authors(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_authors().await?))
}

async fn get_filter_artists(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_artists().await?))
}
