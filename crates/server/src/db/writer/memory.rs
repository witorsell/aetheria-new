use super::*;

impl Writer {
    pub async fn update_chat_memory(
        &self, user_id: i64,
        chat_id: String,
        summary: String,
        up_to_message_id: String,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE chats SET memory_summary = ?, memory_summary_message_id = ? WHERE id = ? AND user_id = ?",
            )
            .bind(&summary)
            .bind(&up_to_message_id)
            .bind(&chat_id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn insert_memory_chunk(
        &self, user_id: i64,
        chat_id: String,
        message_id: String,
        role: String,
        text: String,
        embedding: Vec<u8>,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT OR REPLACE INTO memory_chunks (id, user_id, chat_id, message_id, role, text, embedding, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&chat_id)
            .bind(&message_id)
            .bind(&role)
            .bind(&text)
            .bind(&embedding)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }
}
