-- performance indexes and session idle-timeout tracking

-- indexes for multi-user queries
CREATE INDEX IF NOT EXISTS idx_characters_user_id ON characters(user_id);
CREATE INDEX IF NOT EXISTS idx_messages_user_id_chat_id ON messages(user_id, chat_id);
CREATE INDEX IF NOT EXISTS idx_chats_user_id ON chats(user_id);
CREATE INDEX IF NOT EXISTS idx_chats_character_id ON chats(character_id);
CREATE INDEX IF NOT EXISTS idx_settings_user_id ON settings(user_id);
CREATE INDEX IF NOT EXISTS idx_lorebooks_user_id ON lorebooks(user_id);
CREATE INDEX IF NOT EXISTS idx_lorebook_entries_user_id ON lorebook_entries(user_id);
CREATE INDEX IF NOT EXISTS idx_lorebook_entries_lorebook_id ON lorebook_entries(lorebook_id);
CREATE INDEX IF NOT EXISTS idx_character_lorebooks_character_id ON character_lorebooks(character_id);
CREATE INDEX IF NOT EXISTS idx_chat_lorebooks_chat_id ON chat_lorebooks(chat_id);
CREATE INDEX IF NOT EXISTS idx_presets_user_id ON presets(user_id);
CREATE INDEX IF NOT EXISTS idx_regex_scripts_user_id ON regex_scripts(user_id);
CREATE INDEX IF NOT EXISTS idx_memory_chunks_user_id ON memory_chunks(user_id);
CREATE INDEX IF NOT EXISTS idx_memory_chunks_chat_id ON memory_chunks(chat_id);
CREATE INDEX IF NOT EXISTS idx_group_members_group_id ON group_members(group_id);
CREATE INDEX IF NOT EXISTS idx_group_members_character_id ON group_members(character_id);

-- sessions: idle-timeout tracking
ALTER TABLE sessions ADD COLUMN last_active_at INTEGER;
ALTER TABLE sessions ADD COLUMN issued_at INTEGER;

-- Backfill for sessions created before this migration
UPDATE sessions SET last_active_at = expires_at, issued_at = expires_at;
