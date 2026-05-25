CREATE TABLE source_health (
  source_id INTEGER PRIMARY KEY REFERENCES sources(id) ON DELETE CASCADE,
  last_success_at DATETIME,
  last_error_at DATETIME,
  consecutive_error_count INTEGER NOT NULL DEFAULT 0,
  avg_response_ms REAL
);
