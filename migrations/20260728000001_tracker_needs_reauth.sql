-- Persist refresh rejection or early revocation so sync stops retrying credentials
-- that still appear unexpired and the UI can request relinking.
ALTER TABLE user_tracker_credentials
    ADD COLUMN needs_reauth BOOLEAN NOT NULL DEFAULT FALSE;
