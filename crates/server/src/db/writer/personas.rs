use super::*;

impl Writer {
    pub async fn create_persona(
        &self, user_id: i64, input: crate::models::persona::PersonaInput,
    ) -> sqlx::Result<crate::models::persona::Persona> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let description = input.description.unwrap_or_default();
            sqlx::query(
                "INSERT INTO personas (id, user_id, name, description, avatar_url, created_at, updated_at) VALUES (?, ?, ?, ?, NULL, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&input.name)
            .bind(&description)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::persona::Persona {
                id, user_id, name: input.name, description, avatar_url: None,
                created_at: now, updated_at: now,
            })
        })).await
    }

    pub async fn update_persona(
        &self, user_id: i64, id: String, input: crate::models::persona::PersonaInput,
    ) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            sqlx::query("UPDATE personas SET name = ?, description = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&input.name)
                .bind(input.description.unwrap_or_default())
                .bind(now)
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn update_persona_avatar(&self, user_id: i64, id: String, avatar_url: Option<String>) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE personas SET avatar_url = ? WHERE id = ? AND user_id = ?")
                .bind(&avatar_url)
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    // deletes the persona, and if it was the active one, clears the pointer
    // in the same transaction so users.active_persona_id never dangles
    pub async fn delete_persona(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            let owned = sqlx::query("SELECT 1 FROM personas WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
            if !owned {
                return Ok(false);
            }
            sqlx::query("UPDATE users SET active_persona_id = NULL WHERE id = ? AND active_persona_id = ?")
                .bind(user_id)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM personas WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(true)
        })).await
    }

    // persona_id = None clears the active persona (injection off), same as
    // the old use_persona = false state
    pub async fn set_active_persona(&self, user_id: i64, persona_id: Option<String>) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            if let Some(pid) = &persona_id {
                let owned = sqlx::query("SELECT 1 FROM personas WHERE id = ? AND user_id = ?")
                    .bind(pid)
                    .bind(user_id)
                    .fetch_optional(&mut *conn)
                    .await?
                    .is_some();
                if !owned {
                    return Ok(false);
                }
            }
            sqlx::query("UPDATE users SET active_persona_id = ? WHERE id = ?")
                .bind(&persona_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }
}
