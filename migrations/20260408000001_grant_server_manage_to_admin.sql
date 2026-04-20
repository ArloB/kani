-- Grant server:manage permission to the admin role.
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
VALUES ('admin', 'server:manage');
