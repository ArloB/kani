CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY NOT NULL,
    name TEXT    NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS manga_people (
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('author', 'artist')),
    PRIMARY KEY (manga_id, person_id, role)
);
