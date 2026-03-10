-- Add migration script here
CREATE INDEX idx_manga_tags_tag_id ON manga_tags(tag_id);

CREATE INDEX idx_manga_people_person_id ON manga_people(person_id, role);

CREATE INDEX idx_chapters_download_status ON chapters(download_status);