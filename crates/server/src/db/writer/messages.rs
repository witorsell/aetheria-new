use super::*;
use sqlx::Row;

impl Writer {
    pub async fn create_message(
        &self, user_id: i64,
        chat_id: String,
        parent_id: Option<String>,
        role: String,
        content: String,
    ) -> sqlx::Result<crate::models::message::Message> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO messages (user_id, id, chat_id, parent_id, role, content, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&chat_id)
            .bind(&parent_id)
            .bind(&role)
            .bind(&content)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::message::Message {
                user_id,
                id,
                chat_id,
                parent_id,
                role,
                content,
                visible: true,
                deleted: false,
                created_at: now,
                raw_prompt: None,
                prompt_tokens: None,
                context_limit: None,
                character_id: None,
            })
        })).await
    }

    pub async fn create_assistant_message_with_prompt(
        &self, user_id: i64,
        chat_id: String,
        parent_id: Option<String>,
        content: String,
        raw_prompt: String,
        prompt_tokens: i64,
        context_limit: i64,
        character_id: Option<String>,
    ) -> sqlx::Result<crate::models::message::Message> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO messages (user_id, id, chat_id, parent_id, role, content, created_at, raw_prompt, prompt_tokens, context_limit, character_id) \
                 VALUES (?, ?, ?, ?, 'assistant', ?, ?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&chat_id)
            .bind(&parent_id)
            .bind(&content)
            .bind(now)
            .bind(&raw_prompt)
            .bind(prompt_tokens)
            .bind(context_limit)
            .bind(&character_id)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::message::Message {
                user_id,
                id,
                chat_id,
                parent_id,
                role: "assistant".to_string(),
                content,
                visible: true,
                deleted: false,
                created_at: now,
                raw_prompt: Some(raw_prompt),
                prompt_tokens: Some(prompt_tokens),
                context_limit: Some(context_limit),
                character_id,
            })
        })).await
    }

    pub async fn create_user_message_with_prompt(
        &self, user_id: i64,
        chat_id: String,
        parent_id: Option<String>,
        content: String,
        raw_prompt: String,
        prompt_tokens: i64,
        context_limit: i64,
    ) -> sqlx::Result<crate::models::message::Message> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO messages (user_id, id, chat_id, parent_id, role, content, created_at, raw_prompt, prompt_tokens, context_limit) \
                 VALUES (?, ?, ?, ?, 'user', ?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&chat_id)
            .bind(&parent_id)
            .bind(&content)
            .bind(now)
            .bind(&raw_prompt)
            .bind(prompt_tokens)
            .bind(context_limit)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::message::Message {
                user_id,
                id,
                chat_id,
                parent_id,
                role: "user".to_string(),
                content,
                visible: true,
                deleted: false,
                created_at: now,
                raw_prompt: Some(raw_prompt),
                prompt_tokens: Some(prompt_tokens),
                context_limit: Some(context_limit),
                character_id: None,
            })
        })).await
    }

    pub async fn update_message_content(&self, user_id: i64, id: String, content: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE messages SET content = ? WHERE id = ? AND deleted = 0 AND user_id = ?",
            )
            .bind(&content)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn set_message_visibility(&self, user_id: i64, id: String, visible: bool) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE messages SET visible = ? WHERE id = ? AND deleted = 0 AND user_id = ?",
            )
            .bind(visible as i32)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn soft_delete_message(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            let row = sqlx::query("SELECT parent_id, chat_id, created_at FROM messages WHERE id = ? AND deleted = 0 AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                return Ok(false);
            };
            let parent_id: Option<String> = row.get("parent_id");
            let chat_id: String = row.get("chat_id");
            let created_at: i64 = row.get("created_at");

            invalidate_summary_if_already_folded_in(&mut tx, user_id, &chat_id, created_at).await?;

            sqlx::query("UPDATE messages SET parent_id = ? WHERE parent_id = ?")
                .bind(&parent_id)
                .bind(&id)
                .execute(&mut *tx)
                .await?;

            let deleted = sqlx::query("UPDATE messages SET deleted = 1 WHERE id = ? AND deleted = 0 AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;

            tx.commit().await?;
            Ok(deleted)
        })).await
    }

    pub async fn continue_message(
        &self, user_id: i64,
        id: String,
        appended_content: String,
        raw_prompt: String,
        prompt_tokens: i64,
        context_limit: i64,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE messages SET content = content || ?, raw_prompt = ?, prompt_tokens = ?, context_limit = ? \
                 WHERE id = ? AND role = 'assistant' AND deleted = 0 AND user_id = ?",
            )
            .bind(&appended_content)
            .bind(&raw_prompt)
            .bind(prompt_tokens)
            .bind(context_limit)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn hard_delete_message(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            let row = sqlx::query("SELECT parent_id, chat_id, created_at FROM messages WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                return Ok(false);
            };
            let parent_id: Option<String> = row.get("parent_id");
            let chat_id: String = row.get("chat_id");
            let created_at: i64 = row.get("created_at");

            invalidate_summary_if_already_folded_in(&mut tx, user_id, &chat_id, created_at).await?;

            sqlx::query("UPDATE messages SET parent_id = ? WHERE parent_id = ?")
                .bind(&parent_id)
                .bind(&id)
                .execute(&mut *tx)
                .await?;

            let deleted = sqlx::query("DELETE FROM messages WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;

            tx.commit().await?;
            Ok(deleted)
        })).await
    }
}

/// a chat's memory_summary is one running narrative text with no way to
/// surgically remove a single message's contribution once folded in - if
/// the message being deleted was created at or before whatever
/// memory_summary_message_id currently points to, it's already baked into
/// that narrative (soft-deleting or editing it afterward doesn't touch the
/// summary text), so the only correct move is to invalidate the whole
/// summary and let the next pass rebuild it from what's actually still
/// there. otherwise deleted content - including things like a refusal -
/// keeps getting fed back into every future generation forever.
async fn invalidate_summary_if_already_folded_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: i64,
    chat_id: &str,
    deleted_message_created_at: i64,
) -> sqlx::Result<()> {
    let cursor_created_at: Option<i64> = sqlx::query_scalar(
        "SELECT m.created_at FROM chats c JOIN messages m ON m.id = c.memory_summary_message_id \
         WHERE c.id = ? AND c.user_id = ?",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?;

    if cursor_created_at.is_some_and(|cursor_ts| deleted_message_created_at <= cursor_ts) {
        sqlx::query("UPDATE chats SET memory_summary = NULL, memory_summary_message_id = NULL WHERE id = ? AND user_id = ?")
            .bind(chat_id)
            .bind(user_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
