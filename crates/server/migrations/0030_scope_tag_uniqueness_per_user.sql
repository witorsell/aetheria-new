-- tags.name carried a bare instance-wide UNIQUE constraint from before user_id
-- existed on this table (see 0001_init.sql) - user_id was added later via
-- ALTER TABLE, which doesn't retroactively rescope an existing UNIQUE
-- constraint. rescope it to UNIQUE(user_id, name) so two different users can
-- both have a tag named e.g. "fantasy" without one of them getting a 500.
PRAGMA foreign_keys=off;

CREATE TABLE tags_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
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
