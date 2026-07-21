-- The sole credential for /metrics. Seeded to admin only: /metrics discloses
-- extension names, upstream hosts via the circuit gauge, version and error
-- counts, so it is not a browse-level capability.
INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'metrics:read');
