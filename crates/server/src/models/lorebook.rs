use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Lorebook {
    pub user_id: i64,
    pub id: String,
    pub name: String,
    pub description: String,
    pub scan_depth: i64,
    pub token_budget: i64,
    pub recursive_scanning: bool,
    pub extensions: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LorebookEntry {
    pub user_id: i64,
    pub id: String,
    pub lorebook_id: String,
    pub name: String,
    pub entry: String,
    pub keywords: String, // JSON array of strings
    pub priority: i64,
    pub weight: i64,
    pub enabled: bool,
    pub comment: String,
    pub secondary_keys: String, // JSON array of strings
    pub constant: bool,
    pub position: String,
    pub probability: i64,
    pub use_probability: bool,
    pub selective: bool,
    pub selective_logic: i64,
    pub exclude_recursion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLorebookInput {
    pub user_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub scan_depth: Option<i64>,
    pub token_budget: Option<i64>,
    pub recursive_scanning: Option<bool>,
    pub extensions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLorebookEntryInput {
    pub user_id: Option<i64>,
    pub lorebook_id: String,
    pub name: String,
    pub entry: String,
    pub keywords: Option<String>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub enabled: Option<bool>,
    pub comment: Option<String>,
    pub secondary_keys: Option<String>,
    pub constant: Option<bool>,
    pub position: Option<String>,
    pub probability: Option<i64>,
    pub use_probability: Option<bool>,
    pub selective: Option<bool>,
    pub selective_logic: Option<i64>,
    pub exclude_recursion: Option<bool>,
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Lorebook>> {
    sqlx::query_as::<_, Lorebook>("SELECT * FROM lorebooks WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Lorebook>> {
    sqlx::query_as::<_, Lorebook>("SELECT * FROM lorebooks WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_entries(pool: &sqlx::SqlitePool, user_id: i64, lorebook_id: &str) -> sqlx::Result<Vec<LorebookEntry>> {
    sqlx::query_as::<_, LorebookEntry>("SELECT * FROM lorebook_entries WHERE lorebook_id = ? AND user_id = ? ORDER BY priority DESC, weight DESC")
        .bind(lorebook_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get_entry(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<LorebookEntry>> {
    sqlx::query_as::<_, LorebookEntry>("SELECT * FROM lorebook_entries WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_character_lorebooks(pool: &sqlx::SqlitePool, user_id: i64, character_id: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT lorebook_id FROM character_lorebooks WHERE character_id = ? AND user_id = ?")
        .bind(character_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn list_chat_lorebooks(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>("SELECT lorebook_id FROM chat_lorebooks WHERE chat_id = ? AND user_id = ?")
        .bind(chat_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
}
