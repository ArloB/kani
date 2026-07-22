-- Two columns that were never written and never read.
--
-- `source_circuit_breakers.opened_at`: both upsert sites and the load SELECT
-- omit it, so "when did this breaker open" was unanswerable despite the column
-- existing to answer it.
--
-- `user_manga_tracking.reading_layout`: zero references in Rust *or* JS. The
-- sibling `reading_direction` is fully wired; this half of the per-manga
-- override was never implemented.
ALTER TABLE source_circuit_breakers DROP COLUMN opened_at;
ALTER TABLE user_manga_tracking DROP COLUMN reading_layout;
