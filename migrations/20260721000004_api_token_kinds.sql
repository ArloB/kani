-- Route acceptance keys on token kind rather than scopes. Existing tokens predate API
-- tokens and therefore default to OPDS without gaining access to REST routes.
ALTER TABLE api_tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'opds';

CREATE INDEX IF NOT EXISTS idx_api_tokens_kind ON api_tokens(kind)
    WHERE revoked_at IS NULL;

-- Preserve the existing ability of library viewers to pair an OPDS reader.
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
SELECT role_slug, 'token:create_opds' FROM role_permissions WHERE permission = 'library:view';

-- API credentials require an explicit grant; only administrators receive it by default.
INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'token:create_api');
