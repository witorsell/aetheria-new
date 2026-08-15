use super::*;

impl Writer {
    pub async fn create_lorebook(&self, user_id: i64, input: crate::models::lorebook::CreateLorebookInput) -> sqlx::Result<crate::models::lorebook::Lorebook> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO lorebooks (user_id, id, name, description, scan_depth, token_budget, recursive_scanning, extensions, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(user_id)
            .bind(&id)
            .bind(&input.name)
            .bind(input.description.clone().unwrap_or_default())
            .bind(input.scan_depth.unwrap_or(0))
            .bind(input.token_budget.unwrap_or(0))
            .bind(input.recursive_scanning.unwrap_or(false))
            .bind(input.extensions.clone().unwrap_or_default())
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::lorebook::Lorebook {
                user_id,
                id,
                name: input.name,
                description: input.description.unwrap_or_default(),
                scan_depth: input.scan_depth.unwrap_or(0),
                token_budget: input.token_budget.unwrap_or(0),
                recursive_scanning: input.recursive_scanning.unwrap_or(false),
                extensions: input.extensions.unwrap_or_default(),
                created_at: now,
                updated_at: now,
            })
        })).await
    }

    pub async fn update_lorebook(&self, user_id: i64, id: String, input: crate::models::lorebook::CreateLorebookInput) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            sqlx::query(
                "UPDATE lorebooks SET name = ?, description = ?, scan_depth = ?, token_budget = ?, recursive_scanning = ?, extensions = ?, updated_at = ? WHERE id = ? AND user_id = ?"
            )
            .bind(&input.name)
            .bind(input.description.unwrap_or_default())
            .bind(input.scan_depth.unwrap_or(0))
            .bind(input.token_budget.unwrap_or(0))
            .bind(input.recursive_scanning.unwrap_or(false))
            .bind(input.extensions.unwrap_or_default())
            .bind(now)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn delete_lorebook(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM lorebooks WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn create_lorebook_entry(&self, user_id: i64, input: crate::models::lorebook::CreateLorebookEntryInput) -> sqlx::Result<crate::models::lorebook::LorebookEntry> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO lorebook_entries (user_id, id, lorebook_id, name, entry, keywords, priority, weight, enabled, comment, secondary_keys, constant, position, probability, use_probability, selective, selective_logic, exclude_recursion)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(user_id)
            .bind(&id)
            .bind(&input.lorebook_id)
            .bind(&input.name)
            .bind(&input.entry)
            .bind(input.keywords.clone().unwrap_or_else(|| "[]".to_string()))
            .bind(input.priority.unwrap_or(0))
            .bind(input.weight.unwrap_or(0))
            .bind(input.enabled.unwrap_or(true))
            .bind(input.comment.clone().unwrap_or_default())
            .bind(input.secondary_keys.clone().unwrap_or_else(|| "[]".to_string()))
            .bind(input.constant.unwrap_or(false))
            .bind(input.position.clone().unwrap_or_default())
            .bind(input.probability.unwrap_or(0))
            .bind(input.use_probability.unwrap_or(false))
            .bind(input.selective.unwrap_or(false))
            .bind(input.selective_logic.unwrap_or(0))
            .bind(input.exclude_recursion.unwrap_or(false))
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::lorebook::LorebookEntry {
                user_id,
                id,
                lorebook_id: input.lorebook_id,
                name: input.name,
                entry: input.entry,
                keywords: input.keywords.unwrap_or_else(|| "[]".to_string()),
                priority: input.priority.unwrap_or(0),
                weight: input.weight.unwrap_or(0),
                enabled: input.enabled.unwrap_or(true),
                comment: input.comment.unwrap_or_default(),
                secondary_keys: input.secondary_keys.unwrap_or_else(|| "[]".to_string()),
                constant: input.constant.unwrap_or(false),
                position: input.position.unwrap_or_default(),
                probability: input.probability.unwrap_or(0),
                use_probability: input.use_probability.unwrap_or(false),
                selective: input.selective.unwrap_or(false),
                selective_logic: input.selective_logic.unwrap_or(0),
                exclude_recursion: input.exclude_recursion.unwrap_or(false),
            })
        })).await
    }

    pub async fn update_lorebook_entry(&self, user_id: i64, id: String, input: crate::models::lorebook::CreateLorebookEntryInput) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE lorebook_entries SET name = ?, entry = ?, keywords = ?, priority = ?, weight = ?, enabled = ?, comment = ?, secondary_keys = ?, constant = ?, position = ?, probability = ?, use_probability = ?, selective = ?, selective_logic = ?, exclude_recursion = ? WHERE id = ? AND user_id = ?"
            )
            .bind(&input.name)
            .bind(&input.entry)
            .bind(input.keywords.unwrap_or_else(|| "[]".to_string()))
            .bind(input.priority.unwrap_or(0))
            .bind(input.weight.unwrap_or(0))
            .bind(input.enabled.unwrap_or(true))
            .bind(input.comment.unwrap_or_default())
            .bind(input.secondary_keys.unwrap_or_else(|| "[]".to_string()))
            .bind(input.constant.unwrap_or(false))
            .bind(input.position.unwrap_or_default())
            .bind(input.probability.unwrap_or(0))
            .bind(input.use_probability.unwrap_or(false))
            .bind(input.selective.unwrap_or(false))
            .bind(input.selective_logic.unwrap_or(0))
            .bind(input.exclude_recursion.unwrap_or(false))
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn delete_lorebook_entry(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM lorebook_entries WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn set_character_lorebooks(&self, user_id: i64, character_id: String, lorebook_ids: Vec<String>) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            sqlx::query("DELETE FROM character_lorebooks WHERE character_id = ? AND user_id = ?")
                .bind(&character_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            for lb_id in lorebook_ids {
                sqlx::query("INSERT INTO character_lorebooks (user_id, character_id, lorebook_id) VALUES (?, ?, ?)")
                    .bind(user_id)
                    .bind(&character_id)
                    .bind(&lb_id)
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;
            Ok(())
        })).await
    }

    pub async fn set_chat_lorebooks(&self, user_id: i64, chat_id: String, lorebook_ids: Vec<String>) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            sqlx::query("UPDATE chats SET lorebooks_customized = 1 WHERE id = ? AND user_id = ?")
                .bind(&chat_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM chat_lorebooks WHERE chat_id = ? AND user_id = ?")
                .bind(&chat_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            for lb_id in lorebook_ids {
                sqlx::query("INSERT INTO chat_lorebooks (user_id, chat_id, lorebook_id) VALUES (?, ?, ?)")
                    .bind(user_id)
                    .bind(&chat_id)
                    .bind(&lb_id)
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;
            Ok(())
        })).await
    }
}
