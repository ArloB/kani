-- Axis rules are off, gain-only, or bidirectional. Defaults preserve the existing
-- comparison policy while allowing readers to exclude subjective dimensions such as
-- colour from upgrade decisions.
ALTER TABLE settings ADD COLUMN upgrade_axis_resolution TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_colour TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_encoder TEXT NOT NULL DEFAULT 'both';
ALTER TABLE settings ADD COLUMN upgrade_axis_bitrate TEXT NOT NULL DEFAULT 'gain';

-- Hide downgrade-only comparisons from the replacement queue by default.
ALTER TABLE settings ADD COLUMN upgrade_show_downgrades BOOLEAN NOT NULL DEFAULT 0;

-- Auto-replacement accepts explicit reasons and excludes unmeasured candidates.
ALTER TABLE settings ADD COLUMN upgrade_auto_replace_reasons TEXT NOT NULL
    DEFAULT 'preferred_scanlator,resolution,colour';
