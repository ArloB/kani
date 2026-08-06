-- Records how many newly-discovered chapters the most recent auto-scan filtered
-- out entirely via the manga's download rules. Non-zero drives a dismissable
-- banner on the manga-details page; cleared on dismissal or the next scan that
-- lets any chapter through.
ALTER TABLE manga ADD COLUMN suppressed_chapter_count INTEGER NOT NULL DEFAULT 0;
