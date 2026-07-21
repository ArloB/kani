use crate::jobs::error::JobError;
use crate::jobs::framework::{BackgroundJob, JobContext, JobId, JobPriority};
use crate::service::integrity::ScrubDepth;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ScrubJob {
    id: JobId,
    depth: ScrubDepth,
    fix: bool,
}

impl ScrubJob {
    pub fn new(depth: ScrubDepth, fix: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            depth,
            fix,
        }
    }
}

#[async_trait::async_trait]
impl BackgroundJob for ScrubJob {
    const JOB_TYPE: &'static str = "integrity_scrub";
    type Output = ();

    fn id(&self) -> JobId {
        self.id
    }

    fn description(&self) -> String {
        let depth = match self.depth {
            ScrubDepth::Quick => "Quick",
            ScrubDepth::Deep => "Deep",
        };
        if self.fix {
            format!("{depth} integrity scrub (repairing)")
        } else {
            format!("{depth} integrity scrub")
        }
    }

    fn priority(&self) -> JobPriority {
        JobPriority::Low
    }

    async fn run(self: Box<Self>, ctx: JobContext) -> Result<(), JobError> {
        let svc = ctx.service();
        svc.scrub_library(self.depth, self.fix, Some(ctx.progress.clone()))
            .await
            .map(|_| ())
            .map_err(|e| JobError::Internal(e.to_string()))
    }
}
