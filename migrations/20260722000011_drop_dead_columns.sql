-- Remove fields with no write or read path. Circuit-breaker persistence omits
-- opened_at, while per-manga tracking implements reading_direction but not
-- reading_layout.
ALTER TABLE source_circuit_breakers DROP COLUMN opened_at;
ALTER TABLE user_manga_tracking DROP COLUMN reading_layout;
