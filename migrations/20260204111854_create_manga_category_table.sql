CREATE TABLE IF NOT EXISTS manga_categories (
    manga_id INTEGER NOT NULL,
    category_id INTEGER NOT NULL,
    PRIMARY KEY (manga_id, category_id),
    FOREIGN KEY (manga_id) REFERENCES manga(id),
    FOREIGN KEY (category_id) REFERENCES categories(id)
);
