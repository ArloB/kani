-- Pages as the *source listing* reports them, kept distinct from `page_count`,
-- which records what a downloaded archive turned out to hold.
--
-- Before this the two were the same column, so re-upload detection compared a
-- value against itself: `manifest_capture` writes `page_count` from the CBZ we
-- just built, making the "the source now lists a different count" check
-- structurally incapable of firing for any downloaded chapter.
ALTER TABLE chapters ADD COLUMN source_page_count INTEGER;

CREATE INDEX IF NOT EXISTS idx_chapters_source_page_count
    ON chapters (manga_id)
    WHERE source_page_count IS NOT NULL;
