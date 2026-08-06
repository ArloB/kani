CREATE TABLE webhooks (
    id         INTEGER PRIMARY KEY NOT NULL,
    url        TEXT NOT NULL,
    secret     TEXT,
    events     TEXT NOT NULL DEFAULT '["*"]',
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE webhook_manga_overrides (
    webhook_id INTEGER NOT NULL,
    manga_id   INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (webhook_id, manga_id)
);

CREATE TABLE webhook_deliveries (
    id           INTEGER PRIMARY KEY NOT NULL,
    webhook_id   INTEGER NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    payload      TEXT NOT NULL,
    http_status  INTEGER,
    error        TEXT,
    delivered_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id, delivered_at DESC);
