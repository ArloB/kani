-- Superseded by auto_download_category_ids, the only category setting consumed by
-- library scans. The singular field has no update or read path.
ALTER TABLE settings DROP COLUMN auto_download_category_id;
