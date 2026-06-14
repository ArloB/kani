CREATE TABLE manga_external_ids (
    manga_id    INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,
    external_id TEXT NOT NULL,
    PRIMARY KEY (manga_id, provider)
);

INSERT INTO manga_external_ids (manga_id, provider, external_id)
SELECT id, 'anilist', anilist_id
FROM manga
WHERE anilist_id IS NOT NULL AND anilist_id != '';

INSERT INTO manga_external_ids (manga_id, provider, external_id)
SELECT id, 'mal', mal_id
FROM manga
WHERE mal_id IS NOT NULL AND mal_id != '';

ALTER TABLE manga DROP COLUMN anilist_id;
ALTER TABLE manga DROP COLUMN mal_id;
