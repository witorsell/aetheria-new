#[derive(sqlx::FromRow, Clone)]
pub struct MemoryChunk {
    pub message_id: String,
    pub role: String,
    pub text: String,
    pub embedding: Vec<u8>,
}

pub async fn list_for_chat(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<Vec<MemoryChunk>> {
    sqlx::query_as::<_, MemoryChunk>(
        "SELECT c.message_id, c.role, c.text, c.embedding FROM memory_chunks c JOIN messages m ON m.id = c.message_id AND m.user_id = c.user_id AND m.chat_id = c.chat_id WHERE c.chat_id = ? AND c.user_id = ? AND m.content = c.source_content AND m.visible = 1 AND m.deleted = 0",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn existing_message_ids(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<std::collections::HashSet<String>> {
    sqlx::query_scalar::<_, String>("SELECT c.message_id FROM memory_chunks c JOIN messages m ON m.id = c.message_id AND m.user_id = c.user_id AND m.chat_id = c.chat_id WHERE c.chat_id = ? AND c.user_id = ? AND m.content = c.source_content AND m.visible = 1 AND m.deleted = 0")
        .bind(chat_id)
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map(|v| v.into_iter().collect())
}
