//! Background job management routes.

use super::*;
use kani_app::service::traits::{JobDomain, JobListFilter};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job).delete(cancel_job))
        .route("/jobs/{id}/pause", post(pause_job))
        .route("/jobs/{id}/resume", post(resume_job))
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct JobsQuery {
    pub job_type: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `status` accepts a comma-separated list so the UI can ask for a tab's whole
/// status group in one request (e.g. `status=pending,running`).
fn parse_statuses(raw: Option<&str>) -> Vec<String> {
    raw.map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    })
    .unwrap_or_default()
}

pub(crate) async fn list_jobs(
    _: AuthGuard<crate::permissions::guards::AdminJobs>,
    State(svc): State<Arc<dyn JobDomain>>,
    Query(q): Query<JobsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let filter = JobListFilter {
        job_type: q.job_type.filter(|s| !s.is_empty()),
        statuses: parse_statuses(q.status.as_deref()),
        limit: q.limit.unwrap_or(50).clamp(1, 200),
        offset: q.offset.unwrap_or(0).max(0),
        user_id: None,
    };
    let page = svc.list_jobs(filter).await?;
    Ok(Json(page))
}

#[cfg(test)]
mod tests {
    use super::parse_statuses;

    #[test]
    fn parse_statuses_splits_comma_list() {
        assert_eq!(
            parse_statuses(Some("pending,running")),
            vec!["pending".to_string(), "running".to_string()]
        );
    }

    #[test]
    fn parse_statuses_trims_and_drops_empties() {
        assert_eq!(
            parse_statuses(Some("failed, ,cancelled,")),
            vec!["failed".to_string(), "cancelled".to_string()]
        );
    }

    #[test]
    fn parse_statuses_none_is_empty() {
        assert!(parse_statuses(None).is_empty());
        assert!(parse_statuses(Some("")).is_empty());
    }
}

pub(crate) async fn get_job(
    _: AuthGuard<crate::permissions::guards::AdminJobs>,
    State(svc): State<Arc<dyn JobDomain>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let status = svc.get_job_status(id).await?;
    Ok(Json(status))
}

pub(crate) async fn cancel_job(
    _: AuthGuard<crate::permissions::guards::AdminJobs>,
    State(svc): State<Arc<dyn JobDomain>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AppError> {
    svc.cancel_job(id).await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn pause_job(
    _: AuthGuard<crate::permissions::guards::AdminJobs>,
    State(svc): State<Arc<dyn JobDomain>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let status = svc.get_job_status(id).await?;
    if status.status == "running" || status.status == "pending" {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "pause_not_supported_for_job_type" })),
        )
            .into_response());
    }
    Ok(Json(json!({ "ok": true })).into_response())
}

pub(crate) async fn resume_job(
    _: AuthGuard<crate::permissions::guards::AdminJobs>,
    State(svc): State<Arc<dyn JobDomain>>,
    Path(id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let _status = svc.get_job_status(id).await?;
    Ok((
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "error": "resume_not_supported" })),
    )
        .into_response())
}
