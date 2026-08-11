use super::*;

impl Writer {
    pub async fn create_preset(
        &self, user_id: i64,
        name: String,
        prompts: Vec<crate::models::preset::PresetPrompt>,
        prompt_order: Vec<crate::models::preset::PresetOrderEntry>,
    ) -> sqlx::Result<crate::models::preset::Preset> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let prompts_json = serde_json::to_string(&prompts).unwrap_or_else(|_| "[]".to_string());
            let prompt_order_json = serde_json::to_string(&prompt_order).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "INSERT INTO presets (id, user_id, name, prompts_json, prompt_order_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(user_id)
            .bind(&name)
            .bind(&prompts_json)
            .bind(&prompt_order_json)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::preset::Preset {
                id,
                user_id,
                name,
                prompts,
                prompt_order,
                created_at: now,
                updated_at: now,
            })
        })).await
    }

    pub async fn update_preset_order(
        &self, user_id: i64,
        id: String,
        prompt_order: Vec<crate::models::preset::PresetOrderEntry>,
    ) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            let prompt_order_json = serde_json::to_string(&prompt_order).unwrap_or_else(|_| "[]".to_string());
            sqlx::query("UPDATE presets SET prompt_order_json = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&prompt_order_json)
                .bind(now)
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn delete_preset(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM presets WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn set_active_preset(&self, user_id: i64, preset_id: Option<String>) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE settings SET active_preset_id = ? WHERE user_id = ?")
                .bind(&preset_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        })).await
    }
}
