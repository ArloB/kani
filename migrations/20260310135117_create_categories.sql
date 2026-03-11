CREATE TABLE IF NOT EXISTS categories (
    id         INTEGER PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS manga_categories (
    manga_id    INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    PRIMARY KEY (manga_id, category_id)
);