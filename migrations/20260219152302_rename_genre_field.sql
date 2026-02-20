PRAGMA foreign_keys = OFF;

CREATE TABLE manga_tags_new (
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id)  ON DELETE CASCADE,
    PRIMARY KEY (manga_id, tag_id),
    FOREIGN KEY (manga_id) REFERENCES manga(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

INSERT INTO manga_tags_new (manga_id, tag_id)
SELECT manga_id, genre_id FROM manga_tags;

DROP TABLE manga_tags;
ALTER TABLE manga_tags_new RENAME TO manga_tags;

PRAGMA foreign_keys = ON;
