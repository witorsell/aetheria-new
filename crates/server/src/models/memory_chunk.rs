#[derive(sqlx::FromRow, Clone)]
pub struct MemoryChunk {
    pub message_id: String,
    pub role: String,
    pub text: String,
    pub embedding: Vec<u8>,
}

pub async fn list_for_chat(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<Vec<MemoryChunk>> {
    sqlx::query_as::<_, MemoryChunk>(
        "SELECT message_id, role, text, embedding FROM memory_chunks WHERE chat_id = ? AND user_id = ?",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn existing_message_ids(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<std::collections::HashSet<String>> {
    sqlx::query_scalar::<_, String>("SELECT message_id FROM memory_chunks WHERE chat_id = ? AND user_id = ?")
        .bind(chat_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map(|v| v.into_iter().collect())
}
