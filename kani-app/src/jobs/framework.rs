use std::sync::Arc;

use crate::events::AppEvent;
use crate::jobs::error::JobError;

pub type JobId = uuid::Uuid;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(i64)]
pub enum JobPriority {
    Low = 0,
    Normal = 50,
    High = 100,
}

impl JobPriority {
    pub fn from_i64(v: i64) -> Self {
        match v {
            100 => Self::High,
            50 => Self::Normal,
            _ => Self::Low,
        }
    }
}

#[async_trait::async_trait]
pub trait BackgroundJob: Send + Sync + 'static {
    const JOB_TYPE: &'static str;

    type Output: serde::Serialize + Send;

    fn id(&self) -> JobId;

    fn job_type(&self) -> &'static str {
        Self::JOB_TYPE
    }

    fn description(&self) -> String;

    fn priority(&self) -> JobPriority {
        JobPriority::Normal
    }

    fn source_id(&self) -> Option<i64> {
        None
    }

    fn attempt_count(&self) -> u32 {
        0
    }

    fn retry_params(&self) -> Option<String> {
        None
    }

    async fn run(self: Box<Self>, ctx: JobContext) -> Result<Self::Output, JobError>;

    async fn on_cancel(&self) {}
}

/// Shared cell used to give jobs access to the `AppService` without a circular Arc.
/// The manager populates this after `AppService` is fully constructed.
pub(crate) type ServiceCell = Arc<std::sync::Mutex<Option<crate::service::AppService>>>;

#[derive(Clone)]
pub struct JobContext {
    pub pool: sqlx::sqlite::SqlitePool,
    #[allow(dead_code)]
    pub(crate) sse_tx: tokio::sync::broadcast::Sender<AppEvent>,
    pub cancel: tokio_util::sync::CancellationToken,
    pub progress: JobProgressReporter,
    pub concurrency: JobConcurrencySnapshot,
    pub(crate) svc: ServiceCell,
}

impl JobContext {
    /// Returns a clone of the `AppService` stored in the service cell.
    /// Panics if the service was not set before any job was dispatched.
    pub fn service(&self) -> crate::service::AppService {
        self.svc
            .lock()
            .expect("service cell lock")
            .clone()
            .expect("AppService not set in job service cell")
    }
}

#[derive(Clone)]
pub struct JobConcurrencySnapshot {
    pub page_concurrency: usize,
    pub per_source_download_concurrency: usize,
    pub scan_concurrency: usize,
}

#[derive(Clone, serde::Serialize, Default)]
pub struct JobProgress {
    pub current: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Clone)]
pub struct JobProgressReporter {
    job_id: JobId,
    job_type: &'static str,
    sse_tx: tokio::sync::broadcast::Sender<AppEvent>,
    progress: Arc<tokio::sync::Mutex<JobProgress>>,
}

impl JobProgressReporter {
    pub(crate) fn new(
        job_id: JobId,
        job_type: &'static str,
        sse_tx: tokio::sync::broadcast::Sender<AppEvent>,
    ) -> Self {
        Self {
            job_id,
            job_type,
            sse_tx,
            progress: Arc::new(tokio::sync::Mutex::new(JobProgress::default())),
        }
    }

    pub async fn report(&self, current: u64, total: u64, message: impl Into<String>) {
        let msg = message.into();
        {
            let mut p = self.progress.lock().await;
            p.current = current;
            p.total = total;
            p.message = msg.clone();
        }
        let _ = self.sse_tx.send(AppEvent::JobProgress {
            job_id: self.job_id,
            job_type: self.job_type.to_string(),
            current,
            total,
            message: msg,
        });
    }

    pub async fn current(&self) -> JobProgress {
        self.progress.lock().await.clone()
    }
}
