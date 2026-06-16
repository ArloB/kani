CREATE TABLE source_circuit_breakers (
    source_id INTEGER NOT NULL PRIMARY KEY,
    state TEXT NOT NULL DEFAULT 'closed',
    failure_count INTEGER NOT NULL DEFAULT 0,
    last_failure_at INTEGER,
    opened_at INTEGER,
    next_retry_at INTEGER,
    FOREIGN KEY (source_id) REFERENCES sources(id) ON DELETE CASCADE
);
