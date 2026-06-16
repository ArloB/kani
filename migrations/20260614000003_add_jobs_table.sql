CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT        NOT NULL PRIMARY KEY,
    job_type        TEXT        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending','running','paused','completed','failed','cancelled')),
    priority        INTEGER     NOT NULL DEFAULT 50,
    description     TEXT        NOT NULL DEFAULT '',
    created_at      INTEGER     NOT NULL DEFAULT (unixepoch()),
    started_at      INTEGER,
    completed_at    INTEGER,
    user_id         INTEGER     REFERENCES users(id) ON DELETE SET NULL,
    progress_json   TEXT,
    error_json      TEXT,
    params_json     TEXT,
    result_json     TEXT
);

CREATE INDEX IF NOT EXISTS idx_jobs_status_priority_created
    ON jobs (status, priority DESC, created_at ASC);

CREATE INDEX IF NOT EXISTS idx_jobs_completed_at
    ON jobs (completed_at)
    WHERE status IN ('completed', 'failed', 'cancelled');

CREATE INDEX IF NOT EXISTS idx_jobs_user_id
    ON jobs (user_id, created_at DESC);
