CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY NOT NULL,
    username      TEXT    NOT NULL UNIQUE,
    email         TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    change_id     BLOB    NOT NULL,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_login    DATETIME
);

CREATE TABLE IF NOT EXISTS roles (
    slug        TEXT PRIMARY KEY,
    parent      TEXT REFERENCES roles(slug) ON DELETE CASCADE,
    description TEXT
);

INSERT OR IGNORE INTO roles (slug, description) VALUES
    ('admin', 'Full access to all resources'),
    ('user',  'Standard authenticated user');

CREATE TABLE IF NOT EXISTS user_roles (
    user_id    INTEGER  NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    role_slug  TEXT     NOT NULL REFERENCES roles(slug) ON DELETE CASCADE,
    granted_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    granted_by INTEGER  REFERENCES users(id),
    PRIMARY KEY (user_id, role_slug)
);

CREATE INDEX IF NOT EXISTS idx_user_roles_user ON user_roles(user_id);