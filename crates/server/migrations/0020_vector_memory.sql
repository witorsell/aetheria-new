ALTER TABLE settings ADD COLUMN embedding_source TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN embedding_api_base_url TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN embedding_api_key TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN embedding_model_name TEXT NOT NULL DEFAULT '';

CREATE TABLE memory_chunks (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    chat_id TEXT NOT NULL REFERENCES chats(id),
    message_id TEXT NOT NULL REFERENCES messages(id),
    role TEXT NOT NULL,
    text TEXT NOT NULL,
    embedding BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_memory_chunks_chat_id ON memory_chunks(chat_id);
CREATE UNIQUE INDEX idx_memory_chunks_message_id ON memory_chunks(message_id);
