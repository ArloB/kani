CREATE TABLE user_page_bookmarks (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    page_index INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, chapter_id, page_index)
);

CREATE TABLE user_chapter_notes (
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    note       TEXT    NOT NULL DEFAULT '',
    updated_at TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, chapter_id)
);
