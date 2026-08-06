-- Incremental chapter delivery is host-side and applies to every source. This
-- unwritten flag permanently denied a capability that does not vary by source.
ALTER TABLE sources DROP COLUMN streaming_chapters;
