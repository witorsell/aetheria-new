#[derive(sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub session_secret: String,
    pub display_name: Option<String>,
    pub persona: Option<String>,
    pub use_persona: bool,
    pub avatar_url: Option<String>,
}

pub async fn find_by_username(pool: &sqlx::SqlitePool, username: &str) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, username, password_hash, session_secret, display_name, persona, use_persona, avatar_url FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
}

pub async fn find_by_id(pool: &sqlx::SqlitePool, id: i64) -> sqlx::Result<Option<User>> {
    sqlx::query_as::<_, User>("SELECT id, username, password_hash, session_secret, display_name, persona, use_persona, avatar_url FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}
