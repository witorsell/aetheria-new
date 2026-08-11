PRAGMA foreign_keys=off;

-- Add user_id to messages table (missing from 0011_multi_user.sql)
ALTER TABLE messages ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

-- Add updated_at to alternate_greetings (referenced in INSERT but column doesn't exist)
ALTER TABLE alternate_greetings ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;

-- Add user_id to character_tags (needed for ownership scoping)
ALTER TABLE character_tags ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

-- Add user_id to character_lorebooks (needed for ownership scoping)
ALTER TABLE character_lorebooks ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

-- Add user_id to chat_lorebooks (needed for ownership scoping)
ALTER TABLE chat_lorebooks ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

-- Add user_id to lorebook_entries (needed for ownership scoping)
ALTER TABLE lorebook_entries ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

PRAGMA foreign_keys=on;
