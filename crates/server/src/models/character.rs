use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Character {
    pub user_id: i64,
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_message: String,
    pub avatar_path: Option<String>,
    pub avatar_url: Option<String>,
    pub sample_chat: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub prefill: String,
    pub insert_depth_prompt: String,
    pub insert_depth: i32,
    pub talkativeness: f64,
    pub persona: String,    // JSON
    pub extensions: String, // JSON
    pub folder_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// the fields clients send when creating/updating a character.
/// newer fields are all optional, defaulting to empty strings / null.
#[derive(Deserialize)]
pub struct CharacterInput {
    pub user_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub personality: Option<String>,
    pub scenario: Option<String>,
    pub first_message: Option<String>,
    pub avatar_url: Option<String>,
    pub sample_chat: Option<String>,
    pub system_prompt: Option<String>,
    pub post_history_instructions: Option<String>,
    pub prefill: Option<String>,
    pub insert_depth_prompt: Option<String>,
    pub insert_depth: Option<i32>,
    pub talkativeness: Option<f64>,
    pub persona: Option<String>,
    pub extensions: Option<String>,
    pub folder_id: Option<String>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct AlternateGreeting {
    pub user_id: i64,
    pub id: String,
    pub character_id: String,
    pub greeting: String,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct AlternateGreetingInput {
    pub user_id: Option<i64>,
    pub greeting: String,
}

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Tag {
    pub user_id: i64,
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct TagInput {
    pub user_id: Option<i64>,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Folder {
    pub user_id: i64,
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct FolderInput {
    pub user_id: Option<i64>,
    pub name: String,
    pub parent_id: Option<String>,
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Character>> {
    sqlx::query_as::<_, Character>("SELECT * FROM characters WHERE user_id = ? ORDER BY created_at DESC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Character>> {
    sqlx::query_as::<_, Character>("SELECT * FROM characters WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_alternate_greetings(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    character_id: &str,
) -> sqlx::Result<Vec<AlternateGreeting>> {
    sqlx::query_as::<_, AlternateGreeting>(
        "SELECT * FROM alternate_greetings WHERE character_id = ? AND user_id = ? ORDER BY created_at ASC",
    )
    .bind(character_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn list_tags(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Tag>> {
    sqlx::query_as::<_, Tag>("SELECT * FROM tags WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn list_character_tags(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    character_id: &str,
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT tag_id FROM character_tags WHERE character_id = ? AND user_id = ?",
    )
    .bind(character_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn list_folders(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Folder>> {
    sqlx::query_as::<_, Folder>("SELECT * FROM folders WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}
