-- Per-user session inventory. Mirrors the tower-sessions store with metadata
-- (user_agent, IP, timestamps) for the session management UI.
CREATE TABLE IF NOT EXISTS user_sessions (
    id           TEXT    PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen_at INTEGER NOT NULL DEFAULT (unixepoch()),
    user_agent   TEXT,
    ip_addr      TEXT,
    revoked_at   INTEGER
);

CREATE INDEX idx_user_sessions_user_id ON user_sessions (user_id);
CREATE INDEX idx_user_sessions_active  ON user_sessions (user_id, revoked_at)
    WHERE revoked_at IS NULL;
