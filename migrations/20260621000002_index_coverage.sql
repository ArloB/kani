CREATE INDEX IF NOT EXISTS idx_manga_auto_scan
    ON manga(auto_scan) WHERE auto_scan = 1;

CREATE INDEX IF NOT EXISTS idx_manga_categories_category_id
    ON manga_categories(category_id, manga_id);

CREATE INDEX IF NOT EXISTS idx_chapters_orphaned
    ON chapters(is_orphaned) WHERE is_orphaned = 1;

CREATE INDEX IF NOT EXISTS idx_source_health_source_id
    ON source_health(source_id, last_error_at DESC);

CREATE INDEX IF NOT EXISTS idx_chapters_pending_download
    ON chapters(download_status) WHERE download_status != 2;
