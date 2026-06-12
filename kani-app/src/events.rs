use crate::ids::MangaId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    NewChapters {
        manga_id: MangaId,
        manga_name: String,
        count: usize,
        chapter_ids: Vec<i64>,
        chapter_names: Vec<String>,
    },
    ImportStarted {
        origin: String,
        total: u32,
    },
    ImportProgress {
        origin: String,
        completed: u32,
        total: u32,
        title: String,
    },
    ImportCompleted {
        origin: String,
        imported: u32,
        skipped: u32,
        pending: u32,
    },
    PathMigrationStarted {
        field: String,
        total_bytes: u64,
    },
    PathMigrationProgress {
        field: String,
        bytes_copied: u64,
        total_bytes: u64,
    },
    PathMigrationCompleted {
        field: String,
        new_path: String,
    },
    PathMigrationFailed {
        field: String,
        error: String,
    },
    #[serde(untagged)]
    Refresh(RefreshProgressEvent),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RefreshProgressEvent {
    Started {
        total: usize,
        manga_ids: Vec<MangaId>,
    },
    MangaRefreshed {
        manga_id: MangaId,
        manga_name: String,
        completed: usize,
        total: usize,
        success: bool,
        new_chapters: u32,
    },
    Completed {
        total: usize,
        failed: usize,
    },
}
