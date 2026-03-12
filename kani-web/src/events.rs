use serde::{Deserialize, Serialize};

/// Client-side mirror of `kani_shared::DownloadProgressEvent`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadProgressEvent {
    ChapterStarted {
        chapter_id: i64,
        chapter_name: String,
        total_pages: usize,
    },

    PageCompleted {
        chapter_id: i64,
        chapter_name: String,
        page_index: i32,
    },

    ChapterCompleted {
        chapter_id: i64,
        chapter_name: String,
        successful_pages: usize,
    },

    ChapterFailed {
        chapter_id: i64,
        chapter_name: String,
        error: String,
    },

    ChapterCancelled {
        chapter_id: i64,
        chapter_name: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RefreshProgressEvent {
    Started { total: usize },
    MangaRefreshed {
        manga_id: i64,
        manga_name: String,
        completed: usize,
        total: usize,
        success: bool,
    },
    Completed { total: usize, failed: usize },
}