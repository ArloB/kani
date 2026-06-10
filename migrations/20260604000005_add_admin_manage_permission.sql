-- Grant admin:manage permission to the admin role.
-- This permission gates access to admin-only sensitive routes (user management,
-- extension management, sensitive settings) and is required for 2FA enforcement
-- in public-instance mode.
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
VALUES ('admin', 'admin:manage');
