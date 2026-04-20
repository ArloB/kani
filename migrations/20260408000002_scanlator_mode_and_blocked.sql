-- Add scanlator_mode to manga (priority = all accepted, individual entries can be blocked;
--                               whitelist = only listed scanlators accepted)
ALTER TABLE manga ADD COLUMN scanlator_mode TEXT NOT NULL DEFAULT 'priority';

-- Add blocked flag to scanlator_preferences
-- In priority mode: blocks that specific scanlator entirely.
-- In whitelist mode: ignored (if not in list, already excluded).
ALTER TABLE scanlator_preferences ADD COLUMN blocked INTEGER NOT NULL DEFAULT 0;

-- Migrate existing ScanlatorInclude download rules to scanlator preferences
-- (priority 100, not blocked) so they carry over as preferred scanlators.
INSERT OR IGNORE INTO scanlator_preferences (manga_id, scanlator, priority, blocked)
SELECT manga_id, value, 100, 0
FROM download_rules
WHERE rule_type = 'scanlator_include';

-- Migrate ScanlatorExclude rules to blocked scanlator preferences.
INSERT OR IGNORE INTO scanlator_preferences (manga_id, scanlator, priority, blocked)
SELECT manga_id, value, 0, 1
FROM download_rules
WHERE rule_type = 'scanlator_exclude';

-- Delete the now-redundant scanlator download rules.
DELETE FROM download_rules WHERE rule_type IN ('scanlator_include', 'scanlator_exclude');
