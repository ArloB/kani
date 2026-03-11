CREATE TABLE IF NOT EXISTS manga (
    id          INTEGER PRIMARY KEY NOT NULL,
    source_id   INTEGER    NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    source_manga_id TEXT NOT NULL,
    name        TEXT    NOT NULL,
    cover_url   TEXT,
    description TEXT,
    status      INTEGER NOT NULL CHECK (status IN (0, 1, 2, 3, 4)),
    auto_download BOOLEAN NOT NULL DEFAULT 0,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (source_id, source_manga_id)
);

CREATE INDEX idx_manga_name ON manga(name);

CREATE INDEX idx_manga_updated ON manga(updated_at DESC);

CREATE INDEX idx_manga_source ON manga(source_id);

CREATE TRIGGER manga_updated_at
AFTER UPDATE ON manga
FOR EACH ROW
BEGIN
    UPDATE manga SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;