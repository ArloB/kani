-- Completes the pre-computed quality score so upgrade detection can read it
-- instead of re-parsing `manifest_json`.
--
-- `quality_long_edge` and `quality_bytes_per_mp` were written on every download
-- and read by nothing: `evaluate_upgrades` re-derived the score from the
-- manifest JSON on every scan, for every downloaded chapter. They were also
-- incomplete — the comparator gained colour and encoder-quality axes that the
-- columns could not answer, so reading them was not even possible.
--
-- With these two added, plus the existing `page_count`, the stored columns hold
-- a whole `QualityScore` and the JSON parse can be skipped entirely. On a large
-- library that is one avoided parse per downloaded chapter per scan.
ALTER TABLE chapters ADD COLUMN quality_encoder INTEGER;
ALTER TABLE chapters ADD COLUMN quality_colour TEXT;
