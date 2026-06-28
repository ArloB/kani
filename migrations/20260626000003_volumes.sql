CREATE TABLE IF NOT EXISTS volumes (
    id         INTEGER PRIMARY KEY,
    manga_id   INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    name       TEXT,
    volume_num REAL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_volumes_manga_id ON volumes(manga_id);

ALTER TABLE chapters ADD COLUMN volume_id INTEGER REFERENCES volumes(id) ON DELETE SET NULL;
