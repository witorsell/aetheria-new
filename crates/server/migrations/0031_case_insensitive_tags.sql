-- tag name comparisons were case-sensitive, so "Fantasy" and "fantasy"
-- could exist as two different tags for the same user. rescope name to
-- COLLATE NOCASE so both the UNIQUE(user_id, name) constraint and every
-- ordinary lookup treat them as the same tag.

-- collapse any pre-existing case-variant duplicates per user before the
-- stricter constraint can reject them: keep the earliest-created row per
-- (user_id, lower(name)), remap character_tags references from a
-- duplicate onto the survivor. a character that already had both variants
-- attached would collide on the join table's (character_id, tag_id)
-- primary key here - UPDATE OR IGNORE skips that row rather than erroring,
-- and it gets cleaned up by ON DELETE CASCADE once the duplicate tag itself
-- is deleted below.
CREATE TEMP TABLE tag_dupes AS
SELECT t.id AS dup_id, s.keep_id AS keep_id
FROM tags t
JOIN (
    SELECT user_id, lower(name) AS lname, MIN(id) AS keep_id
    FROM tags
    GROUP BY user_id, lower(name)
) s ON s.user_id = t.user_id AND lower(t.name) = s.lname
WHERE t.id != s.keep_id;

UPDATE OR IGNORE character_tags
SET tag_id = (SELECT keep_id FROM tag_dupes WHERE dup_id = character_tags.tag_id)
WHERE tag_id IN (SELECT dup_id FROM tag_dupes);

DELETE FROM tags WHERE id IN (SELECT dup_id FROM tag_dupes);

DROP TABLE tag_dupes;

PRAGMA foreign_keys=off;

CREATE TABLE tags_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT '#888888',
    created_at INTEGER NOT NULL,
    user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1,
    UNIQUE(user_id, name)
);

INSERT INTO tags_new (id, name, color, created_at, user_id)
SELECT id, name, color, created_at, user_id FROM tags;

DROP TABLE tags;
ALTER TABLE tags_new RENAME TO tags;

PRAGMA foreign_keys=on;
