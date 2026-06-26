CREATE VIRTUAL TABLE IF NOT EXISTS manga_fts USING fts5(
    manga_id UNINDEXED,
    name,
    local_name,
    description,
    authors,
    tokenize = 'unicode61'
);

INSERT INTO manga_fts(manga_id, name, local_name, description, authors)
SELECT
    m.id,
    m.name,
    m.local_name,
    m.description,
    COALESCE((
        SELECT GROUP_CONCAT(n, ' ')
        FROM (
            SELECT p.name AS n
            FROM manga_people mp JOIN people p ON mp.person_id = p.id
            WHERE mp.manga_id = m.id
            UNION ALL
            SELECT name AS n FROM manga_local_authors WHERE manga_id = m.id
        )
    ), '')
FROM manga m;

CREATE TRIGGER manga_fts_insert AFTER INSERT ON manga BEGIN
    INSERT INTO manga_fts(manga_id, name, local_name, description, authors)
    VALUES (NEW.id, NEW.name, NEW.local_name, NEW.description, '');
END;

CREATE TRIGGER manga_fts_update AFTER UPDATE OF name, local_name, description ON manga BEGIN
    DELETE FROM manga_fts WHERE manga_id = OLD.id;
    INSERT INTO manga_fts(manga_id, name, local_name, description, authors)
    VALUES (
        NEW.id,
        NEW.name,
        NEW.local_name,
        NEW.description,
        COALESCE((
            SELECT GROUP_CONCAT(n, ' ')
            FROM (
                SELECT p.name AS n FROM manga_people mp JOIN people p ON mp.person_id = p.id WHERE mp.manga_id = NEW.id
                UNION ALL
                SELECT name AS n FROM manga_local_authors WHERE manga_id = NEW.id
            )
        ), '')
    );
END;

CREATE TRIGGER manga_fts_delete AFTER DELETE ON manga BEGIN
    DELETE FROM manga_fts WHERE manga_id = OLD.id;
END;
