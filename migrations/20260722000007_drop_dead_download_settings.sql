-- `concurrent_manga_downloads` and `chapter_queue_size` were rendered as
-- settings controls, range-validated, persisted, and included in backup
-- export/restore — and read by no code path at any point.
--
-- Real download concurrency comes from `per_source_download_concurrency` and
-- `max_concurrent_jobs`, which already cover what these two claimed to do.
-- `chapter_queue_size` even shipped a tooltip describing deferral behaviour
-- that was never implemented. Removing them is honest; wiring them would have
-- added a third overlapping cap.
ALTER TABLE settings DROP COLUMN concurrent_manga_downloads;
ALTER TABLE settings DROP COLUMN chapter_queue_size;
