CREATE TABLE IF NOT EXISTS extension_cache (
    namespace TEXT NOT NULL,
    key       TEXT NOT NULL,
    value     BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (namespace, key)
);

CREATE INDEX IF NOT EXISTS idx_extension_cache_expires
    ON extension_cache (expires_at)
    WHERE expires_at > 0;
