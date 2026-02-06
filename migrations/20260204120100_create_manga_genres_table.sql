CREATE TABLE IF NOT EXISTS manga_genres (
    manga_id INTEGER NOT NULL,
    genre_id INTEGER NOT NULL,
    PRIMARY KEY (manga_id, genre_id),
    FOREIGN KEY (manga_id) REFERENCES manga(id) ON DELETE CASCADE,
    FOREIGN KEY (genre_id) REFERENCES genres(id) ON DELETE CASCADE
);
