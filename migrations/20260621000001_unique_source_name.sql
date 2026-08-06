-- Source installation treats name as a logical key. Preserve the lowest-ID row and
-- soft-delete renamed duplicates so their foreign-key references survive before the
-- database closes the concurrent-install race with a unique index.
UPDATE sources
SET name = name || '__dup_' || id,
    deleted_at = COALESCE(deleted_at, datetime('now'))
WHERE id NOT IN (SELECT MIN(id) FROM sources GROUP BY name);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sources_name_unique ON sources(name);
