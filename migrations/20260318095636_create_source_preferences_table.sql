CREATE TABLE IF NOT EXISTS source_preferences (
    source_id  INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    key        TEXT    NOT NULL,
    value      TEXT    NOT NULL DEFAULT 'null',
    PRIMARY KEY (source_id, key)
);