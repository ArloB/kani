-- Performance indexes for common lookup patterns.
CREATE INDEX IF NOT EXISTS idx_users_username
    ON users(username);

CREATE INDEX IF NOT EXISTS idx_source_preferences_source_id
    ON source_preferences(source_id);
