-- aetheria consolidated database schema (reference snapshot)
-- contains the complete consolidated schema structure across migrations 0001-0027

CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT,
    persona TEXT,
    use_persona BOOLEAN NOT NULL DEFAULT 0,
    avatar_url TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS characters (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    personality TEXT NOT NULL DEFAULT '',
    scenario TEXT NOT NULL DEFAULT '',
    first_message TEXT NOT NULL DEFAULT '',
    message_example TEXT NOT NULL DEFAULT '',
    creator_notes TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL DEFAULT '',
    post_history_instructions TEXT NOT NULL DEFAULT '',
    avatar_url TEXT,
    talkativeness REAL NOT NULL DEFAULT 0.5,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    character_id TEXT REFERENCES characters(id) ON DELETE SET NULL,
    group_id TEXT REFERENCES groups(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    memory_summary TEXT,
    memory_summary_message_id TEXT,
    lorebooks_customized BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    character_id TEXT,
    raw_prompt TEXT,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    context_limit INTEGER NOT NULL DEFAULT 0,
    visible BOOLEAN NOT NULL DEFAULT 1,
    deleted BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    activation_strategy TEXT NOT NULL DEFAULT 'list',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS group_members (
    group_id TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    character_id TEXT NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    disabled BOOLEAN NOT NULL DEFAULT 0,
    PRIMARY KEY (group_id, character_id)
);

CREATE TABLE IF NOT EXISTS lorebooks (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS lorebook_entries (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    lorebook_id TEXT NOT NULL REFERENCES lorebooks(id) ON DELETE CASCADE,
    keywords TEXT NOT NULL DEFAULT '[]',
    content TEXT NOT NULL DEFAULT '',
    constant BOOLEAN NOT NULL DEFAULT 0,
    position TEXT NOT NULL DEFAULT 'before_char',
    priority INTEGER NOT NULL DEFAULT 10,
    weight INTEGER NOT NULL DEFAULT 100,
    extensions TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    api_base_url TEXT NOT NULL DEFAULT 'https://api.openai.com/v1',
    encrypted_api_key TEXT NOT NULL DEFAULT '',
    model_name TEXT NOT NULL DEFAULT 'gpt-4o',
    system_prompt TEXT NOT NULL DEFAULT '',
    context_limit INTEGER NOT NULL DEFAULT 8192,
    post_history_instructions TEXT NOT NULL DEFAULT '',
    forbid_external_media BOOLEAN NOT NULL DEFAULT 0,
    provider_type TEXT NOT NULL DEFAULT 'openai',
    active_preset_id TEXT,
    summary_provider_type TEXT NOT NULL DEFAULT '',
    summary_api_base_url TEXT NOT NULL DEFAULT '',
    encrypted_summary_api_key TEXT NOT NULL DEFAULT '',
    summary_model_name TEXT NOT NULL DEFAULT '',
    summary_context_limit INTEGER,
    embedding_source TEXT NOT NULL DEFAULT 'local',
    embedding_api_base_url TEXT NOT NULL DEFAULT '',
    encrypted_embedding_api_key TEXT NOT NULL DEFAULT '',
    embedding_model_name TEXT NOT NULL DEFAULT '',
    rag_top_k INTEGER NOT NULL DEFAULT 5,
    rag_score_threshold REAL NOT NULL DEFAULT 0.5,
    temperature REAL NOT NULL DEFAULT 1.0,
    top_p REAL NOT NULL DEFAULT 1.0,
    top_k INTEGER NOT NULL DEFAULT 0,
    frequency_penalty REAL NOT NULL DEFAULT 0.0,
    presence_penalty REAL NOT NULL DEFAULT 0.0,
    max_response_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_effort TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sessions (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    last_accessed_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_chunks (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    text TEXT NOT NULL,
    embedding BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
