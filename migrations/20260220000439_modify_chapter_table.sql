PRAGMA foreign_keys = OFF;

CREATE TABLE chapters_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    manga_id INTEGER NOT NULL,
    chapter_id TEXT NOT NULL,
    source_id INTEGER NOT NULL,
    name TEXT,
    chapter_number REAL NOT NULL,
    language TEXT NOT NULL,
    volume INTEGER,
    scanlator TEXT,
    uploaded_at DATETIME NOT NULL,
    download_status INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (manga_id) REFERENCES manga(id) ON DELETE CASCADE
);

INSERT INTO chapters_new (id, manga_id, name, chapter_number, scanlator, uploaded_at, download_status)
SELECT id, manga_id, name, chapter_number, scanlator, uploaded_at, download_status
FROM chapters;

DROP TABLE chapters;
ALTER TABLE chapters_new RENAME TO chapters;

PRAGMA foreign_keys = ON;