CREATE TABLE IF NOT EXISTS recurring_jobs (
    kind        TEXT NOT NULL PRIMARY KEY,
    last_run_at DATETIME,
    next_due_at DATETIME NOT NULL
);
