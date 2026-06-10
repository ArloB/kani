-- Login attempt tracking for per-identity and per-IP rate limiting.
-- The identity_hash is SHA-256 of the attempted username/email — never stored in plaintext.
CREATE TABLE IF NOT EXISTS login_attempts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_hash TEXT    NOT NULL,
    ip_addr       TEXT    NOT NULL,
    succeeded     BOOLEAN NOT NULL,
    attempted_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_login_attempts_identity ON login_attempts (identity_hash, attempted_at);
CREATE INDEX idx_login_attempts_ip       ON login_attempts (ip_addr,       attempted_at);
