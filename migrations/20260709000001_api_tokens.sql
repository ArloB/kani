CREATE TABLE IF NOT EXISTS api_tokens (
    id           TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    scopes       TEXT NOT NULL DEFAULT 'opds:read opds:progress',
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    last_used_at INTEGER,
    expires_at   INTEGER,
    revoked_at   INTEGER
);

CREATE INDEX idx_api_tokens_user ON api_tokens(user_id) WHERE revoked_at IS NULL;

INSERT OR IGNORE INTO role_permissions (role_slug, permission)
SELECT role_slug, 'opds:read' FROM role_permissions WHERE permission = 'library:view';
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
SELECT role_slug, 'opds:progress' FROM role_permissions WHERE permission = 'library:view';
