-- Superseded by `auto_download_category_ids` (plural), which is what
-- `scan_all_manga` actually reads. The singular column was still SELECTed on
-- boot and shipped in every `GET /rest/settings` response, read by nothing and
-- settable through no update path.
--
-- Removed before the Stage 4 interface freeze rather than after: a meaningless
-- field is a poor thing to promise stability for.
ALTER TABLE settings DROP COLUMN auto_download_category_id;
