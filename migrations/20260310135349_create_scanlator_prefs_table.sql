CREATE TABLE IF NOT EXISTS scanlator_preferences (
    id         INTEGER PRIMARY KEY NOT NULL,
    manga_id   INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    scanlator  TEXT NOT NULL,
    priority   INTEGER NOT NULL DEFAULT 0,
    UNIQUE (manga_id, scanlator)
);