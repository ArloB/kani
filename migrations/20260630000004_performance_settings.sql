ALTER TABLE settings ADD COLUMN max_concurrent_jobs INTEGER NOT NULL DEFAULT 10;
ALTER TABLE settings ADD COLUMN db_maintenance_interval_hours INTEGER NOT NULL DEFAULT 24;
ALTER TABLE settings ADD COLUMN db_vacuum_interval_hours INTEGER NOT NULL DEFAULT 168;
ALTER TABLE settings ADD COLUMN audit_prune_interval_hours INTEGER NOT NULL DEFAULT 168;
ALTER TABLE settings ADD COLUMN trash_purge_interval_hours INTEGER NOT NULL DEFAULT 168;
