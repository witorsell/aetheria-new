use serde::{Deserialize, Serialize};

fn default_role() -> String {
    "system".to_string()
}

fn default_true() -> bool {
    true
}

/// one entry from a SillyTavern completion preset's `prompts` array. `marker`
/// entries (worldInfoBefore, charDescription, chatHistory, ...) have no real
/// `content` of their own: their text is resolved from the live character,
/// lorebook, and history at assembly time instead, see `provider::prompt`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetPrompt {
    pub identifier: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub marker: bool,
    #[serde(default)]
    pub injection_position: i32,
    #[serde(default)]
    pub injection_depth: i32,
}

/// one entry from a preset's `prompt_order` list: which prompt (by
/// identifier) goes where, and whether it's switched on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetOrderEntry {
    pub identifier: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PresetRow {
    id: String,
    user_id: i64,
    name: String,
    prompts_json: String,
    prompt_order_json: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: String,
    pub user_id: i64,
    pub name: String,
    pub prompts: Vec<PresetPrompt>,
    pub prompt_order: Vec<PresetOrderEntry>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// a preset stripped of everything specific to this install (id, owner,
/// timestamps) so it can be saved as a file and re-imported here, or
/// somewhere else, later. `import` accepts this shape directly, so
/// exporting and re-importing round-trips exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetExport {
    pub name: String,
    pub prompts: Vec<PresetPrompt>,
    pub prompt_order: Vec<PresetOrderEntry>,
}

impl From<Preset> for PresetExport {
    fn from(preset: Preset) -> Self {
        PresetExport { name: preset.name, prompts: preset.prompts, prompt_order: preset.prompt_order }
    }
}

impl From<PresetRow> for Preset {
    fn from(row: PresetRow) -> Self {
        Preset {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            prompts: serde_json::from_str(&row.prompts_json).unwrap_or_default(),
            prompt_order: serde_json::from_str(&row.prompt_order_json).unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Preset>> {
    sqlx::query_as::<_, PresetRow>("SELECT * FROM presets WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(Preset::from).collect())
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Preset>> {
    sqlx::query_as::<_, PresetRow>("SELECT * FROM presets WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(Preset::from))
}
