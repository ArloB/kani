-- A tracker link can die while its stored credentials still look valid: the
-- provider rejects the refresh (invalid_grant), or the user revokes the token
-- before `expires_at`. Neither was recorded, so sync retried the same doomed
-- call forever and the UI kept claiming the account was linked.
ALTER TABLE user_tracker_credentials
    ADD COLUMN needs_reauth BOOLEAN NOT NULL DEFAULT FALSE;
