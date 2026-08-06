-- Upgrade detection is library behaviour, not maintenance, so these sit beside
-- the scan settings rather than in the Maintenance group.
ALTER TABLE settings ADD COLUMN upgrade_detection_enabled BOOLEAN NOT NULL DEFAULT 1;
ALTER TABLE settings ADD COLUMN upgrade_min_res_gain REAL NOT NULL DEFAULT 1.2;
-- Confirming a candidate costs a source request, so a scan is bounded per manga.
ALTER TABLE settings ADD COLUMN upgrade_confirm_fetches INTEGER NOT NULL DEFAULT 3;
