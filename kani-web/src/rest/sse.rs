//! Server-sent events and boot-id routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/events", get(combined_sse))
        .route("/boot_id", get(get_boot_id))
}

pub async fn combined_sse(
    AuthGuard(..): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let snapshot = state.downloader.snapshot().await;
    let is_refreshing = state.is_refreshing().await;
    let active_jobs = state.job_manager.active_job_summaries();

    let snapshot_event = Ok::<Event, Infallible>(
        Event::default().data(
            serde_json::json!({
                "type": "state_snapshot",
                "chapters": snapshot,
                "is_refreshing": is_refreshing,
                "active_jobs": active_jobs,
            })
            .to_string(),
        ),
    );

    let download_rx = state.downloader.subscribe();
    let refresh_rx = state.subscribe_refresh();

    let download_stream = BroadcastStream::new(download_rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Download SSE lagged by {} events", n);
            Some(Ok(Event::default().event("close").data("")))
        }
    });

    let refresh_stream = BroadcastStream::new(refresh_rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Refresh SSE lagged by {} events", n);
            None
        }
    });

    let live_stream = download_stream.merge(refresh_stream);
    let stream = tokio_stream::once(snapshot_event).chain(live_stream);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn get_boot_id(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    Json(json!({ "boot_id": state.boot_id }))
}
