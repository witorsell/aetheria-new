use super::*;

impl Writer {
    pub async fn create_group(
        &self, user_id: i64,
        input: crate::models::group::GroupInput,
    ) -> sqlx::Result<crate::models::group::Group> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let activation_strategy = input.activation_strategy.unwrap_or_else(|| "list".to_string());
            sqlx::query(
                "INSERT INTO groups (id, user_id, name, avatar_url, activation_strategy, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&input.name)
            .bind(&input.avatar_url)
            .bind(&activation_strategy)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await?;

            Ok(crate::models::group::Group {
                user_id,
                id,
                name: input.name,
                avatar_url: input.avatar_url,
                activation_strategy,
                created_at: now,
                updated_at: now,
            })
        })).await
    }

    pub async fn update_group(
        &self, user_id: i64,
        id: String,
        input: crate::models::group::GroupInput,
    ) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            let activation_strategy = input.activation_strategy.unwrap_or_else(|| "list".to_string());
            sqlx::query(
                "UPDATE groups SET name = ?, avatar_url = ?, activation_strategy = ?, updated_at = ? WHERE id = ? AND user_id = ?",
            )
            .bind(&input.name)
            .bind(&input.avatar_url)
            .bind(&activation_strategy)
            .bind(now)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn delete_group(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM groups WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn set_group_members(
        &self, user_id: i64,
        group_id: String,
        members: Vec<(String, bool)>,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            sqlx::query("DELETE FROM group_members WHERE group_id = ?")
                .bind(&group_id)
                .execute(&mut *tx)
                .await?;
            for (position, (character_id, disabled)) in members.into_iter().enumerate() {
                sqlx::query(
                    "INSERT INTO group_members (group_id, character_id, position, disabled) VALUES (?, ?, ?, ?)",
                )
                .bind(&group_id)
                .bind(&character_id)
                .bind(position as i64)
                .bind(disabled)
                .execute(&mut *tx)
                .await?;
            }
            let now = chrono_now_millis();
            sqlx::query("UPDATE groups SET updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(now)
                .bind(&group_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        })).await
    }
}
