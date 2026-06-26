ALTER TABLE manga ADD COLUMN cover_hash TEXT;

CREATE TABLE cover_thumbnails (
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    size      TEXT    NOT NULL,
    format    TEXT    NOT NULL,
    path      TEXT    NOT NULL,
    file_size INTEGER NOT NULL,
    created_at TEXT   NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (manga_id, size, format)
);
