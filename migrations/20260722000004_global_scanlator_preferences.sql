-- A NULL manga_id means "applies to the whole library". Per-manga rows still
-- win; globals only fill the gaps.
--
-- SQLite cannot relax NOT NULL in place, so the table is rebuilt. The original
-- UNIQUE(manga_id, scanlator) does not constrain globals at all — SQLite treats
-- NULLs as distinct — so globals get their own partial unique index.
CREATE TABLE scanlator_preferences_new (
    id         INTEGER PRIMARY KEY NOT NULL,
    manga_id   INTEGER REFERENCES manga(id) ON DELETE CASCADE,
    scanlator  TEXT NOT NULL,
    priority   INTEGER NOT NULL DEFAULT 0,
    blocked    BOOLEAN NOT NULL DEFAULT 0,
    UNIQUE (manga_id, scanlator)
);

INSERT INTO scanlator_preferences_new (id, manga_id, scanlator, priority, blocked)
SELECT id, manga_id, scanlator, priority, blocked FROM scanlator_preferences;

DROP TABLE scanlator_preferences;
ALTER TABLE scanlator_preferences_new RENAME TO scanlator_preferences;

CREATE UNIQUE INDEX idx_scanlator_prefs_global
    ON scanlator_preferences(scanlator) WHERE manga_id IS NULL;
CREATE INDEX IF NOT EXISTS idx_scanlator_prefs_manga
    ON scanlator_preferences(manga_id);
