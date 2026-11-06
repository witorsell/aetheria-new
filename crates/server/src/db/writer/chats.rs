use super::*;

impl Writer {
    pub async fn create_chat(
        &self, user_id: i64,
        character_id: Option<String>,
        group_id: Option<String>,
        title: String,
    ) -> sqlx::Result<crate::models::chat::Chat> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            if let Some(id) = &character_id {
                sqlx::query("SELECT 1 FROM characters WHERE id = ? AND user_id = ?")
                    .bind(id).bind(user_id).fetch_one(&mut *tx).await?;
            }
            if let Some(id) = &group_id {
                sqlx::query("SELECT 1 FROM groups WHERE id = ? AND user_id = ?")
                    .bind(id).bind(user_id).fetch_one(&mut *tx).await?;
            }
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
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(crate::models::chat::Chat {
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

impl Writer {
    pub async fn change_chat_member(
        &self, user_id: i64, chat_id: String, character_id: String, add: bool,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            let chat = sqlx::query_as::<_, crate::models::chat::Chat>(
                "SELECT * FROM chats WHERE id = ? AND user_id = ?",
            ).bind(&chat_id).bind(user_id).fetch_one(&mut *tx).await?;
            if add {
                sqlx::query("SELECT 1 FROM characters WHERE id = ? AND user_id = ?")
                    .bind(&character_id).bind(user_id).fetch_one(&mut *tx).await?;
            }
            let mut members: Vec<(String, bool)> = if let Some(group_id) = &chat.group_id {
                sqlx::query("SELECT 1 FROM groups WHERE id = ? AND user_id = ?")
                    .bind(group_id).bind(user_id).fetch_one(&mut *tx).await?;
                sqlx::query_as("SELECT character_id, disabled FROM group_members WHERE group_id = ? ORDER BY position")
                    .bind(group_id).fetch_all(&mut *tx).await?
            } else if let Some(id) = &chat.character_id {
                if !add { return Err(sqlx::Error::Protocol("Chat has no group".into())); }
                vec![(id.clone(), false)]
            } else {
                return Err(sqlx::Error::RowNotFound);
            };
            if add {
                if members.iter().any(|(id, _)| id == &character_id) {
                    return Err(sqlx::Error::Protocol("Character is already in this chat".into()));
                }
                members.push((character_id, false));
            } else {
                if !members.iter().any(|(id, _)| id == &character_id) {
                    return Err(sqlx::Error::RowNotFound);
                }
                members.retain(|(id, _)| id != &character_id);
                if members.is_empty() {
                    return Err(sqlx::Error::Protocol("Cannot remove the last member from a group chat".into()));
                }
            }
            let now = chrono_now_millis();
            if members.len() == 1 {
                sqlx::query("UPDATE chats SET character_id = ?, group_id = NULL, updated_at = ? WHERE id = ? AND user_id = ?")
                    .bind(&members[0].0).bind(now).bind(&chat_id).bind(user_id).execute(&mut *tx).await?;
            } else {
                // copy shared rosters before applying a chat-local edit
                let shared = if let Some(id) = &chat.group_id {
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM chats WHERE group_id = ?")
                        .bind(id).fetch_one(&mut *tx).await? > 1
                } else { false };
                let group_id = if shared || chat.group_id.is_none() {
                    uuid::Uuid::new_v4().to_string()
                } else { chat.group_id.clone().unwrap() };
                if let Some(old_id) = &chat.group_id {
                    if shared {
                        sqlx::query("INSERT INTO groups (id, user_id, name, avatar_url, activation_strategy, created_at, updated_at) SELECT ?, user_id, name, avatar_url, activation_strategy, ?, ? FROM groups WHERE id = ? AND user_id = ?")
                            .bind(&group_id).bind(now).bind(now).bind(old_id).bind(user_id).execute(&mut *tx).await?;
                    } else {
                        sqlx::query("DELETE FROM group_members WHERE group_id = ?")
                            .bind(old_id).execute(&mut *tx).await?;
                        sqlx::query("UPDATE groups SET updated_at = ? WHERE id = ? AND user_id = ?")
                            .bind(now).bind(old_id).bind(user_id).execute(&mut *tx).await?;
                    }
                } else {
                    let names: Vec<String> = sqlx::query_scalar("SELECT name FROM characters WHERE user_id = ? AND (id = ? OR id = ?) ORDER BY name")
                        .bind(user_id).bind(&members[0].0).bind(&members[1].0).fetch_all(&mut *tx).await?;
                    if names.len() != 2 { return Err(sqlx::Error::RowNotFound); }
                    sqlx::query("INSERT INTO groups (id, user_id, name, activation_strategy, created_at, updated_at) VALUES (?, ?, ?, 'list', ?, ?)")
                        .bind(&group_id).bind(user_id).bind(names.join(" & ")).bind(now).bind(now).execute(&mut *tx).await?;
                    sqlx::query("UPDATE messages SET character_id = ? WHERE chat_id = ? AND user_id = ? AND role = 'assistant' AND character_id IS NULL")
                        .bind(&members[0].0).bind(&chat_id).bind(user_id).execute(&mut *tx).await?;
                }
                for (position, (id, disabled)) in members.iter().enumerate() {
                    sqlx::query("INSERT INTO group_members (group_id, character_id, position, disabled) VALUES (?, ?, ?, ?)")
                        .bind(&group_id).bind(id).bind(position as i64).bind(disabled).execute(&mut *tx).await?;
                }
                sqlx::query("UPDATE chats SET character_id = NULL, group_id = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                    .bind(&group_id).bind(now).bind(&chat_id).bind(user_id).execute(&mut *tx).await?;
            }
            if let Some(old_id) = &chat.group_id {
                sqlx::query("DELETE FROM groups WHERE id = ? AND user_id = ? AND NOT EXISTS (SELECT 1 FROM chats WHERE group_id = ?)")
                    .bind(old_id).bind(user_id).bind(old_id).execute(&mut *tx).await?;
            }
            tx.commit().await?;
            Ok(())
        })).await
    }
}
