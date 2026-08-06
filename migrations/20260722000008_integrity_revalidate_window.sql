-- Scheduled scrubs trust recent successful verification for 30 days instead of
-- rehashing the entire library every run. Explicit scrubs still check every file.
ALTER TABLE settings ADD COLUMN integrity_revalidate_after_days INTEGER NOT NULL DEFAULT 30;
