CREATE TABLE IF NOT EXISTS chapters (
    id INTEGER PRIMARY KEY NOT NULL,
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    source_chapter_id TEXT NOT NULL,
    name TEXT,
    chapter_number REAL NOT NULL,
    language TEXT NOT NULL,
    volume INTEGER,
    scanlator TEXT,
    uploaded_at DATETIME,
    download_status INTEGER NOT NULL CHECK (download_status IN (0, 1, 2)) DEFAULT 0,
    discovered_at DATETIME,
    UNIQUE (manga_id, source_chapter_id)
);

CREATE INDEX idx_chapters_manga_number ON chapters(manga_id, chapter_number DESC);