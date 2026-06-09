//! Reading statistics routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stats", get(reading_stats))
        .route("/stats/pace", get(reading_pace_handler))
}

async fn reading_stats(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<crate::models::StatsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let period = q.period.unwrap_or(90);
    let stats = state.get_reading_stats(user.id, period).await?;
    Ok(Json((*stats).clone()))
}

async fn reading_pace_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Query(q): Query<crate::models::PaceQuery>,
) -> Result<impl IntoResponse, AppError> {
    let period = q.period.unwrap_or(90);
    let rows = state.get_reading_pace(user.id, period).await?;
    Ok(Json(rows))
}
