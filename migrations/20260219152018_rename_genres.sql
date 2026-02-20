-- Add migration script here
ALTER TABLE genres RENAME TO tags;
ALTER TABLE manga_genres RENAME TO manga_tags;
