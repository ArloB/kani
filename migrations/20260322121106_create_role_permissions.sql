CREATE TABLE IF NOT EXISTS role_permissions (
    role_slug   TEXT NOT NULL REFERENCES roles(slug) ON DELETE CASCADE,
    permission  TEXT NOT NULL,
    PRIMARY KEY (role_slug, permission)
);

INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('user', 'library:view'),
    ('user', 'library:add'),
    ('user', 'library:delete'),
    ('user', 'chapter:download'),
    ('user', 'chapter:delete'),
    ('user', 'source:browse'),
    ('user', 'settings:view'),
    ('user', 'settings:edit_download'),
    ('user', 'settings:edit_scan'),
    ('user', 'library:refresh'),
    ('user', 'library:manage'),
    ('user', 'source:configure'),

    ('admin', 'source:toggle_enabled'),
    ('admin', 'source:install'),
    ('admin', 'source:delete'),
    ('admin', 'settings:edit_advanced'),
    ('admin', 'user:manage');