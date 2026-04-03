//! Browser-safe type mirrors for `kani_shared` types that appear in `#[server]` function signatures.

pub use kani_shared::types::*;
use serde::Deserialize;



#[derive(Debug, Clone, PartialEq)]
pub enum LiveChapterStatus {
    InProgress,
    Completed,
    CompletedHidden,
    Failed(String),
    Cancelled,
    Deleted,
}

/// Live per-chapter progress tracking for downloads.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterProgress {
    pub id: i64,
    pub name: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub status: LiveChapterStatus,
}

impl ChapterProgress {
    pub fn completion_pct(&self) -> f64 {
        if self.total_pages == 0 {
            return 100.0;
        }
        (self.completed_pages) as f64 / self.total_pages as f64 * 100.0
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveDownloadStatus {
    InProgress,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Deserialize)]
pub struct ActiveDownloadState {
    pub chapter_id:       i64,
    pub chapter_name:     String,
    pub total_pages:      usize,
    pub completed_pages:  usize,
    pub status:           ActiveDownloadStatus,
}

impl From<ActiveDownloadStatus> for LiveChapterStatus {
    fn from(s: ActiveDownloadStatus) -> Self {
        match s {
            ActiveDownloadStatus::InProgress  => LiveChapterStatus::InProgress,
            ActiveDownloadStatus::Completed   => LiveChapterStatus::Completed,
            ActiveDownloadStatus::Failed(e)   => LiveChapterStatus::Failed(e),
            ActiveDownloadStatus::Cancelled   => LiveChapterStatus::Cancelled,
        }
    }
}

impl From<ActiveDownloadState> for ChapterProgress {
    fn from(s: ActiveDownloadState) -> Self {
        ChapterProgress {
            id:              s.chapter_id,
            name:            s.chapter_name,
            total_pages:     s.total_pages,
            completed_pages: s.completed_pages,
            status:          s.status.into(),
        }
    }
}


#[derive(Clone, PartialEq)]
pub enum RefreshState {
    Idle,
    Running { completed: usize, total: usize },
    Done { total: usize, failed: usize },
}

#[derive(Clone, PartialEq)]
pub enum MigrationStep {
    Closed,
    Search,
    Previewing,
    Preview(MigrationPreview, i64, String),
    Confirming,
    Done(MigrationResult),
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[cfg(feature = "ssr")]
pub use crate::auth::User;

#[derive(Clone, Copy, PartialEq)]
pub enum PermissionState {
    Loading,
    Granted,
    Denied,
}
