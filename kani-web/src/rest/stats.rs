//! Reading statistics routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stats", get(reading_stats))
        .route("/stats/pace", get(reading_pace_handler))
}

#[utoipa::path(
    get, path = "/rest/stats",
    params(
        ("period" = Option<i32>, Query, description = "Rolling window in days (default 90, max 365)"),
    ),
    responses(
        (status = 200, description = "Reading statistics: pages read, chapters read, daily activity"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "chapters"
)]
pub(crate) async fn reading_stats(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<crate::models::StatsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let period = q.period.unwrap_or(90);
    let stats = state.get_reading_stats(user.id, period).await?;
    Ok(Json((*stats).clone()))
}

#[utoipa::path(
    get, path = "/rest/stats/pace",
    params(
        ("period" = Option<i32>, Query, description = "Rolling window in days (default 90)"),
    ),
    responses(
        (status = 200, description = "Daily reading pace: chapters-per-day for each day in the period"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "chapters"
)]
pub(crate) async fn reading_pace_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Query(q): Query<crate::models::PaceQuery>,
) -> Result<impl IntoResponse, AppError> {
    let period = q.period.unwrap_or(90);
    let rows = state.get_reading_pace(user.id, period).await?;
    Ok(Json(rows))
}
