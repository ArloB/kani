-- Persistent audit trail for security-sensitive operations.
CREATE TABLE IF NOT EXISTS audit_log (
    id         INTEGER  PRIMARY KEY NOT NULL,
    user_id    INTEGER  REFERENCES users(id) ON DELETE SET NULL,
    action     TEXT     NOT NULL,
    target     TEXT,
    details    TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Fast look-up by user (e.g. "what did this user do?")
CREATE INDEX IF NOT EXISTS idx_audit_log_user_created
    ON audit_log(user_id, created_at DESC);

-- Fast look-up by action type (e.g. "all login failures")
CREATE INDEX IF NOT EXISTS idx_audit_log_action_created
    ON audit_log(action, created_at DESC);
