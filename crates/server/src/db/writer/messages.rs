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

            let row = sqlx::query("SELECT parent_id FROM messages WHERE id = ? AND deleted = 0 AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                return Ok(false);
            };
            let parent_id: Option<String> = row.get("parent_id");

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

            let row = sqlx::query("SELECT parent_id FROM messages WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
            let Some(row) = row else {
                return Ok(false);
            };
            let parent_id: Option<String> = row.get("parent_id");

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
