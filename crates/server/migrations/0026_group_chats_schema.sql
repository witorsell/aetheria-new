-- Which group member said this. NULL for every 1:1 chat message and every
-- user message; set only for assistant replies in a group chat.
ALTER TABLE messages ADD COLUMN character_id TEXT REFERENCES characters(id);

-- Ensure lorebooks_customized exists before the chats table rebuild
-- (it may not exist if this database was created from an earlier migration)
ALTER TABLE chats ADD COLUMN lorebooks_customized BOOLEAN NOT NULL DEFAULT 0;

-- chats: character_id becomes nullable, group_id is new. A chat belongs to
-- exactly one of the two - enforced with a CHECK, not just app code.
CREATE TABLE chats_new (
    id TEXT PRIMARY KEY,
    character_id TEXT REFERENCES characters(id),
    group_id TEXT REFERENCES groups(id),
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1,
    lorebooks_customized BOOLEAN NOT NULL DEFAULT 0,
    memory_summary TEXT,
    memory_summary_message_id TEXT,
    CHECK ((character_id IS NULL) != (group_id IS NULL))
);

INSERT INTO chats_new (id, character_id, group_id, title, created_at, updated_at, user_id, lorebooks_customized, memory_summary, memory_summary_message_id)
SELECT id, character_id, NULL, title, created_at, updated_at, user_id, lorebooks_customized, memory_summary, memory_summary_message_id
FROM chats;

DROP TABLE chats;
ALTER TABLE chats_new RENAME TO chats;

CREATE INDEX idx_chats_character_id ON chats(character_id);
CREATE INDEX idx_chats_group_id ON chats(group_id);
