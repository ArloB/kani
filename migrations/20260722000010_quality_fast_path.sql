-- Complete the precomputed QualityScore so upgrade scans avoid parsing one manifest
-- per downloaded chapter. Existing columns already hold resolution, bitrate, and
-- page count; these add the remaining comparison axes.
ALTER TABLE chapters ADD COLUMN quality_encoder INTEGER;
ALTER TABLE chapters ADD COLUMN quality_colour TEXT;
