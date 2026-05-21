ALTER TABLE settings ADD COLUMN auto_download_category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL;
