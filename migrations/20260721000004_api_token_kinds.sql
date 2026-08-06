-- Separates programmatic API tokens from OPDS reader tokens.
--
-- Route acceptance keys on `kind`, never on scope contents: an OPDS token must
-- never reach /rest/* even if something later writes a broader scope string
-- into its row. Existing tokens were all minted for OPDS readers, so they
-- default to 'opds' and keep working untouched.
ALTER TABLE api_tokens ADD COLUMN kind TEXT NOT NULL DEFAULT 'opds';

CREATE INDEX IF NOT EXISTS idx_api_tokens_kind ON api_tokens(kind)
    WHERE revoked_at IS NULL;

-- Pairing an e-reader is the status quo for anyone who can view the library, so
-- token:create_opds follows the same seeding as opds:read/opds:progress and
-- nobody loses a capability they already had.
INSERT OR IGNORE INTO role_permissions (role_slug, permission)
SELECT role_slug, 'token:create_opds' FROM role_permissions WHERE permission = 'library:view';

-- token:create_api is deliberately NOT seeded broadly. Minting a credential that
-- can act on the REST API with a subset of your permissions is a heavier act
-- than pairing a reader app, and should be granted deliberately. Admins get it
-- so the capability is reachable out of the box.
INSERT OR IGNORE INTO role_permissions (role_slug, permission) VALUES
    ('admin', 'token:create_api');
