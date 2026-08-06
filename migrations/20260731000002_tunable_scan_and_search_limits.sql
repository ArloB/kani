-- Make scan pagination tolerance and per-source global-search timeout adjustable.
-- Defaults preserve three barren pages and use the measured six-second search bound.
ALTER TABLE settings ADD COLUMN scan_barren_page_tolerance INTEGER NOT NULL DEFAULT 3;
ALTER TABLE settings ADD COLUMN global_search_timeout_secs INTEGER NOT NULL DEFAULT 6;
