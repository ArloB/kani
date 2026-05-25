ALTER TABLE users ADD COLUMN email_verified_at DATETIME;

CREATE TABLE email_verification_tokens (
    id          INTEGER PRIMARY KEY NOT NULL,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT    NOT NULL UNIQUE,
    expires_at  DATETIME NOT NULL,
    used_at     DATETIME,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_evt_user       ON email_verification_tokens(user_id);
CREATE INDEX idx_evt_token_hash ON email_verification_tokens(token_hash);
