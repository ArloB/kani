-- These settings have no consumers. Download concurrency is governed by
-- per_source_download_concurrency and max_concurrent_jobs instead.
ALTER TABLE settings DROP COLUMN concurrent_manga_downloads;
ALTER TABLE settings DROP COLUMN chapter_queue_size;
