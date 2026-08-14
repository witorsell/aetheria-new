use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Persona {
    pub id: String,
    pub user_id: i64,
    pub name: String,
    pub description: String,
    pub avatar_url: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct PersonaInput {
    pub name: String,
    pub description: Option<String>,
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Persona>> {
    sqlx::query_as::<_, Persona>("SELECT * FROM personas WHERE user_id = ? ORDER BY created_at ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Persona>> {
    sqlx::query_as::<_, Persona>("SELECT * FROM personas WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}
