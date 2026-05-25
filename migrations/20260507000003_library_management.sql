-- Soft-delete for sources. Setting deleted_at leaves the source row intact so
-- manga.source_id FKs remain valid. Active sources have deleted_at IS NULL.
ALTER TABLE sources ADD COLUMN deleted_at DATETIME;

-- Manga whose source has been soft-deleted are marked orphaned rather than
-- cascade-deleted.
ALTER TABLE manga ADD COLUMN is_orphaned BOOLEAN NOT NULL DEFAULT FALSE;

-- Pending imports queue: manga that couldn't be matched to a Kani source
-- during backup restore or Tachiyomi import, and possible duplicates flagged
-- at import time.
CREATE TABLE IF NOT EXISTS pending_imports (
    id                    INTEGER PRIMARY KEY,
    user_id               INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    origin                TEXT NOT NULL,
    title                 TEXT NOT NULL,
    source_hint           TEXT,
    source_manga_id       TEXT,
    description           TEXT,
    cover_url             TEXT,
    authors               TEXT,
    tags                  TEXT,
    status                INTEGER,
    tracking              TEXT,
    chapter_progress      TEXT,
    possible_duplicate_of INTEGER REFERENCES manga(id) ON DELETE SET NULL,
    duplicate_similarity  REAL,
    resolved              BOOLEAN NOT NULL DEFAULT FALSE,
    created_at            DATETIME DEFAULT CURRENT_TIMESTAMP
);
