-- Per-tracker OAuth app credentials configured by the admin.
-- client_secret is stored plaintext (this is a self-hosted personal app;
-- blast radius of a leak is anime list access only, tokens are revocable).
CREATE TABLE IF NOT EXISTS tracker_app_config (
    tracker_id     INTEGER NOT NULL PRIMARY KEY REFERENCES trackers(id) ON DELETE CASCADE,
    client_id      TEXT    NOT NULL,
    client_secret  TEXT            -- NULL for PKCE-only providers (MAL)
);

-- Server-side PKCE / CSRF state: maps OAuth `state` parameter to the
-- code_verifier (MAL) and the redirect_uri used to build the auth URL.
-- Rows are single-use and expire after 10 minutes (enforced in app logic).
CREATE TABLE IF NOT EXISTS oauth_pkce_state (
    state         TEXT     NOT NULL PRIMARY KEY,
    code_verifier TEXT,            -- NULL for non-PKCE providers (AniList)
    tracker_id    INTEGER  NOT NULL,
    redirect_uri  TEXT     NOT NULL,
    created_at    DATETIME NOT NULL DEFAULT (datetime('now'))
);

-- Global default: whether tracking sync is enabled for new manga.
ALTER TABLE settings ADD COLUMN default_tracking_enabled BOOLEAN NOT NULL DEFAULT TRUE;

-- Per-manga override (TRUE = sync enabled, FALSE = sync disabled).
ALTER TABLE user_manga_tracking ADD COLUMN tracking_enabled BOOLEAN NOT NULL DEFAULT TRUE;
