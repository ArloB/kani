CREATE TABLE repo_trust (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    maintainer_key TEXT NOT NULL,
    trusted_level TEXT NOT NULL DEFAULT 'community',
    last_refreshed_at TEXT,
    index_cache TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'repo:add'),
    ('admin', 'repo:remove'),
    ('admin', 'repo:trust'),
    ('admin', 'repo:refresh');
