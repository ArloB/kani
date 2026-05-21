-- Persisted duplicate pairs detected at manga-add time or via full-library rescan.
-- manga_a_id < manga_b_id is enforced so each pair has exactly one canonical row.
-- ON DELETE CASCADE means removing either manga automatically clears the pair.
CREATE TABLE IF NOT EXISTS duplicate_pairs (
    manga_a_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    manga_b_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    similarity REAL NOT NULL,
    author_match BOOLEAN NOT NULL DEFAULT FALSE,
    dismissed  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (manga_a_id, manga_b_id),
    CHECK (manga_a_id < manga_b_id)
);

CREATE INDEX IF NOT EXISTS idx_duplicate_pairs_b ON duplicate_pairs(manga_b_id);
