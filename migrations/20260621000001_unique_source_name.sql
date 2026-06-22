-- Enforce uniqueness of source names. The install/upsert path treats `name` as a
-- logical key (it reuses an existing row WHERE name = ?), so duplicates should never
-- exist by design; this makes that invariant explicit and closes the install-race
-- TOCTOU window at the database layer.
--
-- Dedupe non-destructively first: keep the lowest id per name, then rename + soft-delete
-- any pre-existing duplicates so foreign-key references (manga, chapters, preferences,
-- health, circuit breakers) remain intact rather than being CASCADE-deleted.
UPDATE sources
SET name = name || '__dup_' || id,
    deleted_at = COALESCE(deleted_at, datetime('now'))
WHERE id NOT IN (SELECT MIN(id) FROM sources GROUP BY name);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sources_name_unique ON sources(name);
