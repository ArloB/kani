use crate::ids::MangaId;
use crate::jobs::error::JobError;
use crate::jobs::framework::{BackgroundJob, JobContext, JobId, JobPriority};
use kani_shared::types::MigrationResult;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct MigrationJob {
    id: JobId,
    pub manga_id: i64,
    pub target_source_id: i64,
    pub target_source_manga_id: String,
    pub keep_orphaned_downloads: bool,
}

impl MigrationJob {
    pub fn new(
        manga_id: MangaId,
        target_source_id: i64,
        target_source_manga_id: String,
        keep_orphaned_downloads: bool,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            manga_id: manga_id.0,
            target_source_id,
            target_source_manga_id,
            keep_orphaned_downloads,
        }
    }
}

#[async_trait::async_trait]
impl BackgroundJob for MigrationJob {
    const JOB_TYPE: &'static str = "migration";
    type Output = MigrationResult;

    fn id(&self) -> JobId {
        self.id
    }

    fn description(&self) -> String {
        format!(
            "Migrate manga {} to source {}",
            self.manga_id, self.target_source_id
        )
    }

    // A migration is a user-initiated action they are waiting on, and it holds
    // the series in a half-moved state until it finishes.
    fn priority(&self) -> JobPriority {
        JobPriority::High
    }

    fn source_id(&self) -> Option<i64> {
        Some(self.target_source_id)
    }

    async fn run(self: Box<Self>, ctx: JobContext) -> Result<MigrationResult, JobError> {
        let svc = ctx.service();
        svc.migrate_manga(
            MangaId(self.manga_id),
            self.target_source_id,
            self.target_source_manga_id,
            self.keep_orphaned_downloads,
        )
        .await
        .map_err(|e| JobError::Internal(e.to_string()))
    }
}
