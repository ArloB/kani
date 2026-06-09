//! Tracker linking, OAuth, mapping & sync routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/manga/{id}/tracking",
            get(get_manga_tracking_handler).put(set_manga_tracking_handler),
        )
        .route("/trackers", get(list_trackers))
        .route("/trackers/{id}/auth_url", get(get_tracker_auth_url))
        .route("/trackers/{id}/callback", get(tracker_oauth_callback))
        .route("/trackers/{id}/unlink", post(unlink_tracker))
        .route("/trackers/{id}/search", get(search_tracker_manga))
        .route(
            "/trackers/{id}/config",
            get(get_tracker_config)
                .put(set_tracker_config)
                .delete(delete_tracker_config),
        )
        .route(
            "/manga/{id}/tracker_mappings",
            get(get_tracker_mappings).put(set_tracker_mapping),
        )
        .route(
            "/manga/{id}/tracker_mappings/{tracker_id}",
            delete(delete_tracker_mapping),
        )
        .route("/trackers/sync", post(sync_all_trackers))
        .route("/manga/{id}/sync", post(sync_manga_trackers))
}

async fn get_manga_tracking_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let tracking = state.get_manga_tracking(user.id, manga_id).await?;
    Ok(Json(tracking))
}

async fn set_manga_tracking_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetMangaTrackingRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(status) = body.status {
        state.set_manga_status(user.id, manga_id, status).await?;
    }
    if let Some(score) = body.score {
        state.set_manga_score(user.id, manga_id, score).await?;
    }
    if let Some(enabled) = body.tracking_enabled {
        state
            .set_manga_tracking_enabled(user.id, manga_id, enabled)
            .await?;
    }
    if let Some(notify) = body.notify_new_chapters {
        state.set_manga_notify(user.id, manga_id, notify).await?;
    }
    if let Some(dir) = body.reading_direction {
        let dir = dir.to_lowercase();
        if dir == "rtl" || dir == "ltr" {
            state.set_reading_direction(user.id, manga_id, &dir).await?;
        }
    }
    if let Some(prefs) = body.reader_prefs {
        state
            .set_reader_prefs(user.id, manga_id, &prefs)
            .await
            .map_err(|_| AppError::ValidationError("reader_prefs must be a JSON object".into()))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.list_trackers_status(user.id).await?;
    let trackers: Vec<_> = items
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "configured": t.configured,
                "linked": t.linked,
            })
        })
        .collect();
    Ok(Json(trackers))
}

async fn get_tracker_auth_url(
    AuthGuard(..): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerAuthUrlQuery>,
) -> Result<impl IntoResponse, AppError> {
    let url = state
        .get_tracker_auth_url(tracker_id, &q.redirect_uri)
        .await?;
    Ok(Json(json!({ "url": url })))
}

async fn tracker_oauth_callback(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerCallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    state
        .complete_tracker_oauth(user.id, tracker_id, &q.code, &q.state)
        .await?;
    Ok((
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        OAUTH_SUCCESS_HTML,
    ))
}

async fn unlink_tracker(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.unlink_tracker(user.id, tracker_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn search_tracker_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerSearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let results = state
        .search_tracker_manga(user.id, tracker_id, &q.query)
        .await?;
    Ok(Json(results))
}

async fn get_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let config = state.get_tracker_config(tracker_id).await?;
    match config {
        Some((client_id, secret_configured)) => Ok(Json(json!({
            "client_id": client_id,
            "secret_configured": secret_configured,
        }))),
        None => Ok(Json(json!({
            "client_id": null,
            "secret_configured": false,
        }))),
    }
}

async fn set_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Json(body): Json<SetTrackerConfigRequest>,
) -> Result<impl IntoResponse, AppError> {
    let secret = body.client_secret.as_deref().filter(|s| !s.is_empty());
    state
        .set_tracker_config(tracker_id, &body.client_id, secret)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_tracker_config(tracker_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_tracker_mappings(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let mappings = state.get_tracker_mappings(user.id, manga_id).await?;
    let response: Vec<_> = mappings
        .into_iter()
        .map(|m| {
            json!({
                "tracker_id": m.tracker_id,
                "tracker_name": m.tracker_name,
                "tracker_manga_id": m.tracker_manga_id,
            })
        })
        .collect();
    Ok(Json(response))
}

async fn set_tracker_mapping(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetTrackerMappingRequest>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::trackers::set_mapping(
        &state.db,
        user.id,
        body.tracker_id,
        manga_id,
        &body.tracker_manga_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tracker_mapping(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path((manga_id, tracker_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::trackers::delete_mapping(&state.db, user.id, tracker_id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_all_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.sync_all_trackers(user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_manga_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.sync_manga_trackers(user.id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
