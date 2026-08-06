-- External error reporting and its feature have been removed. Local metrics, logs,
-- diagnostics, and support bundles remain available without this setting.
ALTER TABLE settings DROP COLUMN error_reporting_enabled;
