CREATE TABLE personas (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    avatar_url TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_personas_user_id ON personas(user_id);

ALTER TABLE users ADD COLUMN active_persona_id TEXT REFERENCES personas(id);

-- backfill existing single-persona field into persona rows so nobody loses their text
INSERT INTO personas (id, user_id, name, description, avatar_url, created_at, updated_at)
SELECT lower(hex(randomblob(16))), id, 'Default', persona, NULL,
       CAST(strftime('%s','now') AS INTEGER) * 1000,
       CAST(strftime('%s','now') AS INTEGER) * 1000
FROM users
WHERE use_persona = 1 AND persona IS NOT NULL AND trim(persona) != '';

UPDATE users SET active_persona_id = (
    SELECT p.id FROM personas p WHERE p.user_id = users.id AND p.name = 'Default'
)
WHERE use_persona = 1 AND persona IS NOT NULL AND trim(persona) != '';

-- users.persona / users.use_persona left in place, not dropped
