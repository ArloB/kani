CREATE TABLE IF NOT EXISTS user_chapter_tracking (
    user_id        INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id     INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    is_read        BOOLEAN NOT NULL DEFAULT FALSE,
    last_page_read INTEGER NOT NULL DEFAULT 0,
    last_read_at   DATETIME,
    PRIMARY KEY (user_id, chapter_id)
);

CREATE INDEX IF NOT EXISTS idx_user_chapter_recent ON user_chapter_tracking(user_id, last_read_at DESC);

CREATE TABLE IF NOT EXISTS user_manga_tracking (
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    status   INTEGER NOT NULL DEFAULT 0 CHECK (status IN (0, 1, 2, 3, 4, 5)),
    score    REAL,
    PRIMARY KEY (user_id, manga_id)
);

CREATE TABLE IF NOT EXISTS trackers (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS user_tracker_credentials (
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tracker_id    INTEGER NOT NULL REFERENCES trackers(id) ON DELETE CASCADE,
    access_token  TEXT,
    refresh_token TEXT,
    expires_at    DATETIME,
    PRIMARY KEY (user_id, tracker_id)
);

CREATE TABLE IF NOT EXISTS tracker_manga_mappings (
    user_id          INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tracker_id       INTEGER NOT NULL REFERENCES trackers(id) ON DELETE CASCADE,
    manga_id         INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    tracker_manga_id TEXT NOT NULL,
    PRIMARY KEY (user_id, tracker_id, manga_id)
);