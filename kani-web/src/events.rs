use serde::{Deserialize, Serialize};

/// Client-side mirror of `kani_shared::DownloadProgressEvent`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadProgressEvent {
    ChapterStarted {
        chapter_name: String,
        total_pages: usize,
    },

    PageCompleted {
        chapter_name: String,
        page_index: i32,
    },

    PageFailed {
        chapter_name: String,
        page_index: i32,
        error: String,
    },

    ChapterCompleted {
        chapter_name: String,
        successful_pages: usize,
        failed_pages: usize,
    },

    ChapterFailed {
        chapter_name: String,
        error: String,
    },
}
