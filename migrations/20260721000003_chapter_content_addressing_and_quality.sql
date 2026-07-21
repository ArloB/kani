-- Content addressing + quality tracking for downloaded chapters.
--
-- file_path is stored RELATIVE to library_path so relocating the library (or
-- path_migration.rs rewriting it) never invalidates these rows. Today the
-- on-disk path is re-derived from the manga title at read time, which means
-- renaming a manga silently orphans its files; a stored path removes that
-- entire failure class.
ALTER TABLE chapters ADD COLUMN file_path TEXT;
ALTER TABLE chapters ADD COLUMN content_hash TEXT;
ALTER TABLE chapters ADD COLUMN manifest_json TEXT;
ALTER TABLE chapters ADD COLUMN file_verified_at INTEGER;
ALTER TABLE chapters ADD COLUMN quality_long_edge INTEGER;
ALTER TABLE chapters ADD COLUMN quality_bytes_per_mp REAL;
-- JSON UpgradeCandidate descriptor, or NULL when no upgrade is pending.
ALTER TABLE chapters ADD COLUMN upgrade_available TEXT;

ALTER TABLE manga ADD COLUMN upgrade_auto_replace BOOLEAN NOT NULL DEFAULT 0;

-- Partial index: only hashed chapters participate in exact-duplicate detection.
CREATE INDEX IF NOT EXISTS idx_chapters_content_hash ON chapters(content_hash)
    WHERE content_hash IS NOT NULL;

-- Last N scrub reports, so the admin UI survives a restart.
CREATE TABLE IF NOT EXISTS scrub_reports (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    depth       TEXT NOT NULL,
    report_json TEXT NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_scrub_reports_created ON scrub_reports(created_at DESC);
