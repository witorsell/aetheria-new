-- Phase 3: Character Depth & Editor

-- New columns for the characters table
ALTER TABLE characters ADD COLUMN avatar_url TEXT;
ALTER TABLE characters ADD COLUMN sample_chat TEXT NOT NULL DEFAULT '';
ALTER TABLE characters ADD COLUMN system_prompt TEXT NOT NULL DEFAULT '';
ALTER TABLE characters ADD COLUMN post_history_instructions TEXT NOT NULL DEFAULT '';
ALTER TABLE characters ADD COLUMN prefill TEXT NOT NULL DEFAULT '';
ALTER TABLE characters ADD COLUMN insert_depth_prompt TEXT NOT NULL DEFAULT '';
ALTER TABLE characters ADD COLUMN persona TEXT NOT NULL DEFAULT '{}';        -- JSON: { kind, attributes }
ALTER TABLE characters ADD COLUMN extensions TEXT NOT NULL DEFAULT '{}';     -- JSON: { voice, sprite, accentColor, ... }

-- Alternate greetings (multiple first messages for a character)
CREATE TABLE IF NOT EXISTS alternate_greetings (
    id TEXT PRIMARY KEY,
    character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    greeting TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_alt_greetings_char ON alternate_greetings(character_id);

-- Tags
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT NOT NULL DEFAULT '#888888',
    created_at INTEGER NOT NULL
);

-- Character <-> Tag join table
CREATE TABLE IF NOT EXISTS character_tags (
    character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (character_id, tag_id)
);

-- Folders (self-referencing for hierarchy)
CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

-- Folder assignment for characters
ALTER TABLE characters ADD COLUMN folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL;
