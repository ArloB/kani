-- Scrub cadence is runtime-tunable, so it lives in Settings rather than env
-- vars. Defaults match the recurring kinds' built-in intervals: a cheap hash
-- pass daily, the per-page pass weekly.
ALTER TABLE settings ADD COLUMN integrity_quick_scrub_interval_hours INTEGER NOT NULL DEFAULT 24;
ALTER TABLE settings ADD COLUMN integrity_deep_scrub_interval_hours INTEGER NOT NULL DEFAULT 168;
ALTER TABLE settings ADD COLUMN scrub_on_startup BOOLEAN NOT NULL DEFAULT 0;
