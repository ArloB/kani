CREATE TABLE IF NOT EXISTS smart_collections (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    rule_json  TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);
