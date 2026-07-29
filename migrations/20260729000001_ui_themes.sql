-- Server-persisted UI themes (plan 05). A theme is a set of design-token
-- overrides plus optional custom CSS. `user_id IS NULL` marks an instance-wide
-- theme published by an admin and visible to everyone.
--
-- This is a table rather than a Settings column on purpose: it is a collection
-- with per-user ownership, so the 8-step settings pattern does not apply.
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
