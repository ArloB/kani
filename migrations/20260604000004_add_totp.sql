-- TOTP (Time-based One-Time Password) configuration per user.
-- The secret is stored encrypted via CredentialCipher; the plaintext is base32.
-- `verified_at` is NULL until the user completes setup verification.
CREATE TABLE IF NOT EXISTS user_totp (
    user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret      TEXT    NOT NULL,
    verified_at INTEGER,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Single-use backup codes; argon2id hash stored, not plaintext.
CREATE TABLE IF NOT EXISTS user_backup_codes (
    id        TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(8)))),
    user_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT    NOT NULL,
    used_at   INTEGER
);

CREATE INDEX idx_backup_codes_user ON user_backup_codes (user_id, used_at)
    WHERE used_at IS NULL;
