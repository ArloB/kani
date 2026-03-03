//! Handlers for download management and real-time progress events.

use axum::{
    Router,
    extract::State,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use std::convert::Infallible;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use crate::{error::AppError, state::AppState};

pub async fn download_progress_sse(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rx = state.downloader.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }

        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("SSE client lagged, skipped {} download progress events", n);
            None
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/downloads/progress", get(download_progress_sse))
}
