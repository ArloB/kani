-- OPDS-PSE page numbering.
--
-- Kani served `?page=` as a 0-based index into the CBZ while advertising the
-- standard `{pageNumber}` template, so readers that substitute a 1-based number
-- (the common reading of the spec) were off by one: page 1 showed page 2, and
-- the final page 404'd. The default is now 1-based; operators whose reader sends
-- 0-based numbers can switch back, the same choice Komga exposes.
ALTER TABLE settings ADD COLUMN opds_page_index_zero_based BOOLEAN NOT NULL DEFAULT 0;
