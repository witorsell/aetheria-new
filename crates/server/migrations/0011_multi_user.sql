PRAGMA foreign_keys=off;

-- Recreate users table to remove CHECK (id = 1)
CREATE TABLE users_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    session_secret TEXT NOT NULL,
    display_name TEXT,
    persona TEXT,
    use_persona BOOLEAN NOT NULL DEFAULT 0
);
INSERT INTO users_new SELECT id, username, password_hash, session_secret, display_name, persona, use_persona FROM users;
DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

-- Insert a default user (id=1) if it doesn't exist, so FK constraints on other tables are satisfied
INSERT OR IGNORE INTO users (id, username, password_hash, session_secret) VALUES (1, 'admin', '', '');

-- Add user_id to all tables
ALTER TABLE characters ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;
ALTER TABLE chats ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;
ALTER TABLE folders ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;
ALTER TABLE tags ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;
ALTER TABLE lorebooks ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;
ALTER TABLE sessions ADD COLUMN user_id INTEGER NOT NULL REFERENCES users(id) DEFAULT 1;

-- Recreate settings table to remove CHECK (id = 1) and make it per-user
CREATE TABLE settings_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    api_base_url TEXT NOT NULL DEFAULT '',
    api_key TEXT NOT NULL DEFAULT '',
    model_name TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL DEFAULT '',
    context_limit INTEGER NOT NULL DEFAULT 8192,
    post_history_instructions TEXT NOT NULL DEFAULT '',
    forbid_external_media BOOLEAN NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);
INSERT INTO settings_new (id, user_id, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, updated_at)
SELECT id, 1, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, updated_at FROM settings;
DROP TABLE settings;
ALTER TABLE settings_new RENAME TO settings;

PRAGMA foreign_keys=on;
