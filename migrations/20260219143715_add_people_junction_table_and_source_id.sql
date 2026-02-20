PRAGMA foreign_keys = OFF;

CREATE TABLE manga_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id   TEXT    NOT NULL,
    source      INTEGER NOT NULL,
    name        TEXT    NOT NULL,
    cover_url   TEXT    NOT NULL,
    description TEXT    NOT NULL,
    status      INTEGER NOT NULL,
    auto_download BOOLEAN NOT NULL DEFAULT 0,
    library_path  TEXT    NOT NULL,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO manga_new (id, source_id, source, name, cover_url, description, status, auto_download, library_path, created_at, updated_at)
SELECT id, CAST(source_id AS TEXT), source_id, name, cover_url, description, status, auto_download, library_path, created_at, updated_at
FROM manga;

DROP TABLE manga;
ALTER TABLE manga_new RENAME TO manga;

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS manga_authors (
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (manga_id, person_id)
);

CREATE TABLE IF NOT EXISTS manga_artists (
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    PRIMARY KEY (manga_id, person_id)
);
