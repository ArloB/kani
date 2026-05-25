ALTER TABLE manga ADD COLUMN local_name        TEXT;
ALTER TABLE manga ADD COLUMN local_description TEXT;
ALTER TABLE manga ADD COLUMN local_status      INTEGER;
ALTER TABLE manga ADD COLUMN cover_overridden  BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS manga_local_authors (
    id       INTEGER PRIMARY KEY NOT NULL,
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    name     TEXT    NOT NULL,
    role     TEXT    NOT NULL CHECK (role IN ('author', 'artist'))
);
CREATE INDEX IF NOT EXISTS idx_mla_manga ON manga_local_authors(manga_id);

CREATE TABLE IF NOT EXISTS manga_local_tags (
    id       INTEGER PRIMARY KEY NOT NULL,
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    name     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mlt_manga ON manga_local_tags(manga_id);
