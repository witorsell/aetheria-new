CREATE TABLE IF NOT EXISTS presets (
    id TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    prompts_json TEXT NOT NULL,
    prompt_order_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS regex_scripts (
    id TEXT PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    script_name TEXT NOT NULL,
    find_regex TEXT NOT NULL,
    replace_string TEXT NOT NULL DEFAULT '',
    trim_strings_json TEXT NOT NULL DEFAULT '[]',
    placement_json TEXT NOT NULL DEFAULT '[]',
    disabled BOOLEAN NOT NULL DEFAULT FALSE,
    markdown_only BOOLEAN NOT NULL DEFAULT FALSE,
    prompt_only BOOLEAN NOT NULL DEFAULT FALSE,
    run_on_edit BOOLEAN NOT NULL DEFAULT FALSE,
    substitute_regex INTEGER NOT NULL DEFAULT 0,
    min_depth INTEGER,
    max_depth INTEGER,
    created_at INTEGER NOT NULL
);

ALTER TABLE settings ADD COLUMN active_preset_id TEXT;
