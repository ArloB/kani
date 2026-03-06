//! Progress tracking types for the download manager.
//!
//! This module provides types for tracking download progress.
//! These are skeleton implementations that can be expanded to support
//! UI progress indicators, logging, or other monitoring needs.

/// Represents the current state of a download
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub chapter_name: String,
    pub total_pages: usize,
    pub completed_pages: usize,
    pub failed_pages: usize,
}

impl DownloadProgress {
    pub fn new(chapter_name: String, total_pages: usize) -> Self {
        Self {
            chapter_name,
            total_pages,
            completed_pages: 0,
            failed_pages: 0,
        }
    }

    /// Returns the completion percentage (0.0 to 1.0)
    pub fn completion_ratio(&self) -> f64 {
        if self.total_pages == 0 {
            return 1.0;
        }
        (self.completed_pages + self.failed_pages) as f64 / self.total_pages as f64
    }

    /// Returns true if the download is complete (all pages processed)
    pub fn is_complete(&self) -> bool {
        self.completed_pages + self.failed_pages >= self.total_pages
    }

    /// Returns true if any pages failed
    pub fn has_errors(&self) -> bool {
        self.failed_pages > 0
    }
}

/// Events emitted during download progress
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// A chapter download has started
    ChapterStarted {
        chapter_id: i64,
        chapter_name: String,
        total_pages: usize,
    },

    /// A single page was downloaded successfully
    PageCompleted {
        chapter_id: i64,
        chapter_name: String,
        page_index: i32,
    },

    /// A chapter download completed (all pages processed)
    ChapterCompleted {
        chapter_id: i64,
        chapter_name: String,
        successful_pages: usize,
        failed_pages: usize,
    },

    /// A chapter download failed completely (e.g., couldn't create directory)
    ChapterFailed {
        chapter_id: i64,
        chapter_name: String,
        error: String,
    },

    /// A chapter download was cancelled
    ChapterCancelled {
        chapter_id: i64,
        chapter_name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_progress_new() {
        let progress = DownloadProgress::new("Chapter 1".to_string(), 10);
        assert_eq!(progress.chapter_name, "Chapter 1");
        assert_eq!(progress.total_pages, 10);
        assert_eq!(progress.completed_pages, 0);
        assert_eq!(progress.failed_pages, 0);
    }

    #[test]
    fn test_completion_ratio() {
        let mut progress = DownloadProgress::new("Chapter 1".to_string(), 10);
        assert_eq!(progress.completion_ratio(), 0.0);

        progress.completed_pages = 5;
        assert_eq!(progress.completion_ratio(), 0.5);

        progress.failed_pages = 2;
        assert_eq!(progress.completion_ratio(), 0.7);
    }

    #[test]
    fn test_is_complete() {
        let mut progress = DownloadProgress::new("Chapter 1".to_string(), 10);
        assert!(!progress.is_complete());

        progress.completed_pages = 8;
        progress.failed_pages = 2;
        assert!(progress.is_complete());
    }

    #[test]
    fn test_has_errors() {
        let mut progress = DownloadProgress::new("Chapter 1".to_string(), 10);
        assert!(!progress.has_errors());

        progress.failed_pages = 1;
        assert!(progress.has_errors());
    }
}
