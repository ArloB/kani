ALTER TABLE user_manga_tracking ADD COLUMN reading_layout INTEGER NOT NULL DEFAULT 0;
ALTER TABLE user_manga_tracking ADD COLUMN reading_direction TEXT NOT NULL DEFAULT 'rtl';
