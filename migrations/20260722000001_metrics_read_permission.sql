-- Lets a scraper authenticate with a scoped API token instead of the shared
-- KANI_METRICS_TOKEN. Seeded to admin only: /metrics discloses extension names,
-- upstream hosts via the circuit gauge, version and error counts.
INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'metrics:read');
