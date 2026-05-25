-- Add Mihon/Tachiyomi source ID to sources table for cross-app import matching.
ALTER TABLE sources ADD COLUMN mihon_source_id INTEGER;
CREATE INDEX IF NOT EXISTS idx_sources_mihon_source_id ON sources (mihon_source_id) WHERE mihon_source_id IS NOT NULL;
