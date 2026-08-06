-- Grants access to sensitive administration routes and public-instance 2FA enforcement.
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
VALUES ('admin', 'admin:manage');
