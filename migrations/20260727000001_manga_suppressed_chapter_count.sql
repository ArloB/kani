-- Records chapters rejected by download rules during the latest auto-scan. The UI
-- clears the signal on dismissal or after a scan accepts a chapter.
ALTER TABLE manga ADD COLUMN suppressed_chapter_count INTEGER NOT NULL DEFAULT 0;
