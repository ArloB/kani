-- Keep the source listing's count separate from the downloaded archive's page_count;
-- re-upload detection compares these independent values.
ALTER TABLE chapters ADD COLUMN source_page_count INTEGER;

CREATE INDEX IF NOT EXISTS idx_chapters_source_page_count
    ON chapters (manga_id)
    WHERE source_page_count IS NOT NULL;
