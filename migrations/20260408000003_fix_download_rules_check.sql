-- Recreate the table with an updated CHECK constraint
CREATE TABLE download_rules_new (
    id        INTEGER PRIMARY KEY NOT NULL,
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    rule_type TEXT NOT NULL CHECK (rule_type IN (
        'scanlator_include', 'scanlator_exclude',
        'language_include',  'language_exclude',
        'title_contains',    'title_excludes',
        'chapter_number_min', 'chapter_number_max',
        'exclude_fractional', 'max_age_days',
        'published_after'
    )),
    value     TEXT NOT NULL
);

INSERT INTO download_rules_new (id, manga_id, rule_type, value)
SELECT id, manga_id, rule_type, value FROM download_rules;

DROP TABLE download_rules;
ALTER TABLE download_rules_new RENAME TO download_rules;
CREATE INDEX idx_download_rules_manga ON download_rules(manga_id);
