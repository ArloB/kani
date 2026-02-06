CREATE TABLE IF NOT EXISTS chapters (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    manga_id INTEGER NOT NULL,
    url TEXT NOT NULL,
    name TEXT NOT NULL,
    cover_url TEXT NOT NULL,
    chapter_number REAL NOT NULL,
    scanlator TEXT NOT NULL,
    uploaded_at DATETIME NOT NULL,
    downloaded_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    download_status INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (manga_id) REFERENCES manga(id)
);
