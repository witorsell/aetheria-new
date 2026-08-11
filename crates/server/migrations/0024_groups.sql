CREATE TABLE groups (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    avatar_url TEXT,
    activation_strategy TEXT NOT NULL DEFAULT 'list',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE group_members (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    character_id TEXT NOT NULL REFERENCES characters(id),
    position INTEGER NOT NULL,
    disabled BOOLEAN NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, character_id)
);

CREATE INDEX idx_group_members_group_id ON group_members(group_id);
