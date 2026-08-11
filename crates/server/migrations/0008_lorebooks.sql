CREATE TABLE IF NOT EXISTS lorebooks (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    scan_depth INTEGER NOT NULL DEFAULT 5,
    token_budget INTEGER NOT NULL DEFAULT 2048,
    recursive_scanning BOOLEAN NOT NULL DEFAULT FALSE,
    extensions TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS lorebook_entries (
    id TEXT PRIMARY KEY NOT NULL,
    lorebook_id TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    entry TEXT NOT NULL,
    keywords TEXT NOT NULL DEFAULT '[]',
    priority INTEGER NOT NULL DEFAULT 10,
    weight INTEGER NOT NULL DEFAULT 10,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    comment TEXT NOT NULL DEFAULT '',
    secondary_keys TEXT NOT NULL DEFAULT '[]',
    constant BOOLEAN NOT NULL DEFAULT FALSE,
    position TEXT NOT NULL DEFAULT 'before_char',
    probability INTEGER NOT NULL DEFAULT 100,
    use_probability BOOLEAN NOT NULL DEFAULT FALSE,
    selective BOOLEAN NOT NULL DEFAULT FALSE,
    selective_logic INTEGER NOT NULL DEFAULT 0,
    exclude_recursion BOOLEAN NOT NULL DEFAULT FALSE,
    FOREIGN KEY (lorebook_id) REFERENCES lorebooks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS character_lorebooks (
    character_id TEXT NOT NULL,
    lorebook_id TEXT NOT NULL,
    PRIMARY KEY (character_id, lorebook_id),
    FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE,
    FOREIGN KEY (lorebook_id) REFERENCES lorebooks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS chat_lorebooks (
    chat_id TEXT NOT NULL,
    lorebook_id TEXT NOT NULL,
    PRIMARY KEY (chat_id, lorebook_id),
    FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    FOREIGN KEY (lorebook_id) REFERENCES lorebooks(id) ON DELETE CASCADE
);
