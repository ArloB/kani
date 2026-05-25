use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    NewChapters {
        manga_id: i64,
        manga_name: String,
        count: usize,
        chapter_names: Vec<String>,
    },
    #[serde(untagged)]
    Refresh(RefreshProgressEvent),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RefreshProgressEvent {
    Started {
        total: usize,
    },
    MangaRefreshed {
        manga_id: i64,
        manga_name: String,
        completed: usize,
        total: usize,
        success: bool,
    },
    Completed {
        total: usize,
        failed: usize,
    },
}

