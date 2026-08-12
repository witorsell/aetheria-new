CREATE TABLE themes (
    id TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL REFERENCES users(id),
    name TEXT NOT NULL,
    token_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_themes_user_id ON themes(user_id);

-- 'default' or 'light' select a built-in (compiled-in) theme; any other
-- value is looked up in the themes table above, falling back to 'default'
-- if the row is missing (e.g. it was deleted on another device).
ALTER TABLE settings ADD COLUMN active_theme_id TEXT NOT NULL DEFAULT 'default';
