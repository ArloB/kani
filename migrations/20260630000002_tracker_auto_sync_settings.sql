ALTER TABLE settings ADD COLUMN tracker_auto_sync_enabled BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE settings ADD COLUMN tracker_sync_interval_hours INTEGER NOT NULL DEFAULT 24;
