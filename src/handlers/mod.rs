mod download_handlers;
mod manga_handlers;
mod source_handlers;

use axum::{
    Router,
    extract::{Query, State},
    http::header,
    response::IntoResponse,
    routing::get,
};

use crate::{error::AppError, models::ProxyQuery, state::AppState};

pub async fn image_proxy(
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> Result<impl IntoResponse, AppError> {
    if kani_core::network::is_private_host(&query.url) {
        return Err(AppError::InternalServerError(
            "Proxy request blocked: target is a private or reserved address".to_string(),
        ));
    }

    let response = state
        .http_client
        .get(&query.url)
        .header("Referer", &query.referer)
        .send()
        .await?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .cloned()
        .ok_or_else(|| AppError::InternalServerError("Failed to get content type".to_string()))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    Ok((
        status,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
        ],
        bytes,
    ))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/image_proxy", get(image_proxy))
        .merge(download_handlers::routes())
        .merge(source_handlers::routes())
        .merge(manga_handlers::routes())
}
