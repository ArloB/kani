PRAGMA foreign_keys = OFF;

CREATE TABLE manga_new (
    id          INTEGER PRIMARY KEY NOT NULL,
    source_id   INTEGER    NOT NULL,
    source_manga_id TEXT NOT NULL,
    name        TEXT    NOT NULL,
    cover_url   TEXT,
    description TEXT,
    status      INTEGER NOT NULL,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (source_id) REFERENCES sources(id) ON DELETE CASCADE,
    UNIQUE (source_id, source_manga_id)
);

CREATE INDEX idx_manga_name ON manga_new(name);

CREATE INDEX idx_manga_updated ON manga_new(updated_at DESC);

CREATE INDEX idx_manga_source ON manga_new(source_id);

INSERT INTO manga_new (id, source_id, name, cover_url, description, status, created_at, updated_at)
SELECT id, source_id, name, cover_url, description, status, created_at, updated_at
FROM manga;

DROP TABLE manga;
ALTER TABLE manga_new RENAME TO manga;

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY NOT NULL,
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
