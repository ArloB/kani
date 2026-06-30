use crate::error::Result;
use crate::service::AppService;
use time::OffsetDateTime;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type,
)]
#[sqlx(type_name = "TEXT")]
pub enum RecurringJobKind {
    DbMaintenance,
    DbVacuum,
    AutoScan,
    AuditPrune,
    TrashPurge,
    StorageMonitor,
    PendingDeleteRetry,
    ScheduledBackup,
    TrackerSync,
}

impl RecurringJobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DbMaintenance => "db_maintenance",
            Self::DbVacuum => "db_vacuum",
            Self::AutoScan => "auto_scan",
            Self::AuditPrune => "audit_prune",
            Self::TrashPurge => "trash_purge",
            Self::StorageMonitor => "storage_monitor",
            Self::PendingDeleteRetry => "pending_delete_retry",
            Self::ScheduledBackup => "scheduled_backup",
            Self::TrackerSync => "tracker_sync",
        }
    }

    fn default_interval_secs(self) -> i64 {
        match self {
            Self::DbMaintenance => 24 * 60 * 60,
            Self::DbVacuum => 7 * 24 * 60 * 60,
            Self::AutoScan => 60 * 60,
            Self::AuditPrune => 7 * 24 * 60 * 60,
            Self::TrashPurge => 7 * 24 * 60 * 60,
            Self::StorageMonitor => 24 * 60 * 60,
            Self::PendingDeleteRetry => 60 * 60,
            Self::ScheduledBackup => 60 * 60,
            Self::TrackerSync => 60 * 60,
        }
    }

    pub fn all() -> &'static [Self] {
        &[
            Self::DbMaintenance,
            Self::DbVacuum,
            Self::AutoScan,
            Self::AuditPrune,
            Self::TrashPurge,
            Self::StorageMonitor,
            Self::PendingDeleteRetry,
            Self::ScheduledBackup,
            Self::TrackerSync,
        ]
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::all().iter().copied().find(|k| k.as_str() == s)
    }
}

pub async fn ensure_recurring_rows(pool: &sqlx::SqlitePool) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    for kind in RecurringJobKind::all() {
        let kind_str = kind.as_str();
        let interval = kind.default_interval_secs();
        let next_due = now + time::Duration::seconds(interval);
        sqlx::query!(
            "INSERT OR IGNORE INTO recurring_jobs (kind, last_run_at, next_due_at) \
             VALUES (?, NULL, ?)",
            kind_str,
            next_due
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn record_run(
    pool: &sqlx::SqlitePool,
    kind: RecurringJobKind,
    interval_override: Option<i64>,
) -> Result<()> {
    let now = OffsetDateTime::now_utc();
    let interval = interval_override.unwrap_or_else(|| kind.default_interval_secs());
    let next_due = now + time::Duration::seconds(interval);
    let kind_str = kind.as_str();
    sqlx::query!(
        "UPDATE recurring_jobs SET last_run_at = ?, next_due_at = ? WHERE kind = ?",
        now,
        next_due,
        kind_str
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn is_due(pool: &sqlx::SqlitePool, kind: RecurringJobKind) -> bool {
    let kind_str = kind.as_str();
    let now = OffsetDateTime::now_utc();
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM recurring_jobs WHERE kind = ? AND next_due_at <= ?",
        kind_str,
        now
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false)
}

async fn auto_scan_job_active(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM jobs WHERE job_type = 'auto_scan' AND status IN ('pending', 'running')"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false)
}

async fn scheduled_backup_job_active(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM jobs WHERE job_type = 'scheduled_backup' AND status IN ('pending', 'running')"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false)
}

async fn tracker_sync_job_active(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar!(
        "SELECT COUNT(*) FROM jobs WHERE job_type = 'tracker_sync' AND status IN ('pending', 'running')"
    )
    .fetch_one(pool)
    .await
    .map(|c| c > 0)
    .unwrap_or(false)
}

async fn scheduled_backup_time_matches(svc: &AppService) -> bool {
    let Ok(config) = svc.get_backup_schedule().await else {
        return false;
    };
    if !config.enabled {
        return false;
    }
    let now = OffsetDateTime::now_utc();
    let current_hour = now.hour();
    let current_weekday = now.weekday().number_days_from_monday();
    match config.frequency {
        crate::service::backup_scheduler::BackupFrequency::Daily { hour } => current_hour == hour,
        crate::service::backup_scheduler::BackupFrequency::Weekly { weekday, hour } => {
            current_weekday == weekday && current_hour == hour
        }
    }
}

async fn run_kind(svc: &AppService, kind: RecurringJobKind) {
    if kind == RecurringJobKind::AutoScan && auto_scan_job_active(&svc.db).await {
        return;
    }

    if kind == RecurringJobKind::ScheduledBackup {
        if !scheduled_backup_time_matches(svc).await {
            if let Err(e) = record_run(&svc.db, kind, None).await {
                tracing::warn!(
                    "Failed to record recurring job reschedule for {:?}: {e}",
                    kind
                );
            }
            return;
        }
        if scheduled_backup_job_active(&svc.db).await {
            return;
        }
    }

    if kind == RecurringJobKind::TrackerSync {
        let (enabled, interval_hours) = {
            let s = svc.settings.read().await;
            (s.tracker_auto_sync_enabled, s.tracker_sync_interval_hours)
        };
        if !enabled {
            if let Err(e) = record_run(&svc.db, kind, Some(interval_hours * 60 * 60)).await {
                tracing::warn!(
                    "Failed to record recurring job reschedule for {:?}: {e}",
                    kind
                );
            }
            return;
        }
        if tracker_sync_job_active(&svc.db).await {
            return;
        }
    }

    let result = submit_kind_job(svc, kind).await.map(|_| ());

    match result {
        Ok(()) => {
            let interval_override = if kind == RecurringJobKind::AutoScan {
                let mins = svc.settings.read().await.scan_interval_minutes;
                Some(mins * 60)
            } else if kind == RecurringJobKind::TrackerSync {
                let hours = svc.settings.read().await.tracker_sync_interval_hours;
                Some(hours * 60 * 60)
            } else {
                None
            };
            if let Err(e) = record_run(&svc.db, kind, interval_override).await {
                tracing::warn!("Failed to record recurring job run for {:?}: {e}", kind);
            }
        }
        Err(e) => tracing::warn!("Recurring job {:?} failed: {e}", kind),
    }
}

/// Submits the background job that backs a recurring kind, returning its id.
/// Shared by the scheduler and the manual trigger path.
async fn submit_kind_job(
    svc: &AppService,
    kind: RecurringJobKind,
) -> Result<crate::jobs::JobId> {
    match kind {
        RecurringJobKind::DbMaintenance => {
            svc.job_manager
                .submit(crate::jobs::maintenance::AnalyzeJob::new())
                .await
        }
        RecurringJobKind::DbVacuum => {
            svc.job_manager
                .submit(crate::jobs::maintenance::VacuumJob::new())
                .await
        }
        RecurringJobKind::AutoScan => {
            svc.job_manager
                .submit(crate::jobs::scan::AutoScanJob::new())
                .await
        }
        RecurringJobKind::AuditPrune => {
            svc.job_manager
                .submit(crate::jobs::audit_prune::AuditPruneJob::new())
                .await
        }
        RecurringJobKind::TrashPurge => {
            let days = svc.settings.read().await.trash_retention_days.max(0) as u32;
            svc.job_manager
                .submit(crate::jobs::trash_purge::TrashPurgeJob::new(days))
                .await
        }
        RecurringJobKind::StorageMonitor => {
            svc.job_manager
                .submit(crate::jobs::storage::StorageMonitorJob::new())
                .await
        }
        RecurringJobKind::PendingDeleteRetry => {
            svc.job_manager
                .submit(crate::jobs::pending_delete_retry::PendingDeleteRetryJob::new())
                .await
        }
        RecurringJobKind::ScheduledBackup => {
            svc.job_manager
                .submit(crate::jobs::backup::ScheduledBackupJob::new())
                .await
        }
        RecurringJobKind::TrackerSync => {
            svc.job_manager
                .submit(crate::jobs::tracker_sync::TrackerSyncJob::new())
                .await
        }
    }
}

/// Manually triggers a recurring kind's job immediately, bypassing the schedule
/// (does not touch `next_due_at`). Skips with `Ok(None)` if an instance of a
/// singleton kind is already pending/running. Used by the admin trigger endpoint.
pub async fn trigger_now(
    svc: &AppService,
    kind: RecurringJobKind,
) -> Result<Option<crate::jobs::JobId>> {
    let already_active = match kind {
        RecurringJobKind::AutoScan => auto_scan_job_active(&svc.db).await,
        RecurringJobKind::ScheduledBackup => scheduled_backup_job_active(&svc.db).await,
        RecurringJobKind::TrackerSync => tracker_sync_job_active(&svc.db).await,
        _ => false,
    };
    if already_active {
        return Ok(None);
    }
    let id = submit_kind_job(svc, kind).await?;
    Ok(Some(id))
}

pub fn spawn_recurring_scheduler(svc: &AppService) {
    let svc = svc.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = svc.shutdown_token.cancelled() => break,
                _ = interval.tick() => {}
            }
            for &kind in RecurringJobKind::all() {
                if is_due(&svc.db, kind).await {
                    run_kind(&svc, kind).await;
                }
            }
        }
    });
}
