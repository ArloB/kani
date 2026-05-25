ALTER TABLE settings ADD COLUMN registration_enabled BOOLEAN NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS captcha_challenges (
    id TEXT PRIMARY KEY,
    answer INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
