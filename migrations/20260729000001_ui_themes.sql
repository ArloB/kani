-- Themes are owned collections of token overrides and optional CSS rather than a
-- singleton setting. A NULL user_id marks an administrator-published instance theme.
CREATE TABLE IF NOT EXISTS ui_themes (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    user_id     INTEGER REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    tokens_json TEXT NOT NULL,
    custom_css  TEXT,
    is_active   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_ui_themes_user ON ui_themes(user_id);

INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'theme:publish');
