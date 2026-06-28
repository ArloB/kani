CREATE TABLE IF NOT EXISTS storage_history (
    id                  INTEGER PRIMARY KEY,
    captured_at         DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    library_used_bytes  INTEGER NOT NULL DEFAULT 0,
    cover_used_bytes    INTEGER NOT NULL DEFAULT 0,
    chapter_used_bytes  INTEGER NOT NULL DEFAULT 0,
    data_used_bytes     INTEGER NOT NULL DEFAULT 0,
    free_bytes          INTEGER NOT NULL DEFAULT 0,
    total_manga         INTEGER NOT NULL DEFAULT 0,
    total_chapters      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_storage_history_captured_at ON storage_history(captured_at);
