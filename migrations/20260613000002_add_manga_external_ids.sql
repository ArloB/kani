CREATE TABLE manga_external_ids (
    manga_id    INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,
    external_id TEXT NOT NULL,
    PRIMARY KEY (manga_id, provider)
);
