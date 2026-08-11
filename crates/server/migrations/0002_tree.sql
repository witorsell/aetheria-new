ALTER TABLE messages ADD COLUMN parent_id TEXT REFERENCES messages(id);
ALTER TABLE messages ADD COLUMN visible INTEGER NOT NULL DEFAULT 1;
ALTER TABLE messages ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;

-- Populate parent_id for existing messages: each message's parent is the
-- previous message in the same chat, by created_at order. The first message
-- in each chat gets parent_id = NULL.
UPDATE messages SET parent_id = (
    SELECT m2.id FROM messages m2
    WHERE m2.chat_id = messages.chat_id
      AND m2.created_at < messages.created_at
      AND m2.deleted = 0
    ORDER BY m2.created_at DESC
    LIMIT 1
);
