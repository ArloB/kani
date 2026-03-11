CREATE TABLE IF NOT EXISTS download_rules (
    id        INTEGER PRIMARY KEY NOT NULL,
    manga_id  INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    rule_type TEXT NOT NULL CHECK (rule_type IN (
        'scanlator_include', 'scanlator_exclude',
        'language_include',  'language_exclude',
        'title_contains',    'title_excludes'
    )),
    value     TEXT NOT NULL
);
CREATE INDEX idx_download_rules_manga ON download_rules(manga_id);