use super::*;

impl Writer {
    pub async fn create_chat(
        &self, user_id: i64,
        character_id: Option<String>,
        group_id: Option<String>,
        title: String,
    ) -> sqlx::Result<crate::models::chat::Chat> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO chats (user_id, id, character_id, group_id, title, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&character_id)
            .bind(&group_id)
            .bind(&title)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::chat::Chat {
                user_id, id, character_id, group_id, title,
                created_at: now, updated_at: now,
                memory_summary: None, memory_summary_message_id: None,
            })
        })).await
    }

    pub async fn convert_chat_to_group(&self, user_id: i64, chat_id: String, group_id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            sqlx::query("UPDATE chats SET character_id = NULL, group_id = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&group_id)
                .bind(now)
                .bind(&chat_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn convert_chat_to_direct(&self, user_id: i64, chat_id: String, character_id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            sqlx::query("UPDATE chats SET character_id = ?, group_id = NULL, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&character_id)
                .bind(now)
                .bind(&chat_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn convert_chat_to_group_with_new_member(
        &self,
        user_id: i64,
        chat_id: String,
        existing_character_id: String,
        new_character_id: String,
        group_name: String,
    ) -> sqlx::Result<crate::models::group::Group> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            let now = chrono_now_millis();
            let group_id = uuid::Uuid::new_v4().to_string();
            let activation_strategy = "list".to_string();

            sqlx::query(
                "INSERT INTO groups (id, user_id, name, avatar_url, activation_strategy, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&group_id)
            .bind(user_id)
            .bind(&group_name)
            .bind(None::<String>)
            .bind(&activation_strategy)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO group_members (group_id, character_id, position, disabled) VALUES (?, ?, 0, 0)",
            )
            .bind(&group_id)
            .bind(&existing_character_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO group_members (group_id, character_id, position, disabled) VALUES (?, ?, 1, 0)",
            )
            .bind(&group_id)
            .bind(&new_character_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "UPDATE messages SET character_id = ? WHERE chat_id = ? AND role = 'assistant' AND character_id IS NULL",
            )
            .bind(&existing_character_id)
            .bind(&chat_id)
            .execute(&mut *tx)
            .await?;

            sqlx::query("UPDATE chats SET character_id = NULL, group_id = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&group_id)
                .bind(now)
                .bind(&chat_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;

            Ok(crate::models::group::Group {
                user_id,
                id: group_id,
                name: group_name,
                avatar_url: None,
                activation_strategy,
                created_at: now,
                updated_at: now,
            })
        })).await
    }
}
