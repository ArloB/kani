-- Makes upgrade judgement configurable per axis, and gives auto-replace
-- something to read.
--
-- Each axis takes one of 'off' (never decides anything), 'gain' (an improvement
-- here is an upgrade, a regression is ignored) or 'both' (a regression also
-- blocks the candidate). The defaults reproduce the previously hardcoded
-- behaviour exactly: resolution, colour and encoder regressions blocked a
-- candidate; a bitrate drop never did.
--
-- 'colour' is the axis that motivated this: a colour release is not universally
-- preferable, and some readers want the original monochrome scan.
ALTER TABLE settings ADD COLUMN upgrade_axis_resolution TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_colour TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_encoder TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_bitrate TEXT NOT NULL DEFAULT 'gain';

-- Reassurance entries ("yours is better") are noise in a list whose purpose is
-- deciding what to replace, so the library-wide view hides them unless asked.
ALTER TABLE settings ADD COLUMN upgrade_show_downgrades BOOLEAN NOT NULL DEFAULT 0;

-- Which candidates `manga.upgrade_auto_replace` may act on, as a CSV of
-- `preferred_scanlator` plus any QualityReason. Deliberately excludes
-- `unmeasured` — replacing a file because nothing could be measured is not a
-- decision anyone asked for.
ALTER TABLE settings ADD COLUMN upgrade_auto_replace_reasons TEXT NOT NULL
    DEFAULT 'preferred_scanlator,resolution,colour';
