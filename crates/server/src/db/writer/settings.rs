use super::*;

impl Writer {
    pub async fn touch_settings(&self) -> sqlx::Result<()> {
        self.dispatch(|conn| Box::pin(async move {
            let now = chrono_now_millis();
            let result = sqlx::query("UPDATE settings SET updated_at = ? WHERE user_id = (SELECT MAX(id) FROM users)")
                .bind(now)
                .execute(&mut *conn)
                .await;
            let needs_insert = match &result {
                Ok(res) => res.rows_affected() == 0,
                Err(_) => true,
            };
            if needs_insert {
                let _ = sqlx::query(
                    "INSERT OR IGNORE INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, updated_at) VALUES ((SELECT MAX(id) FROM users), '', '', '', '', 8192, '', 0, ?)",
                )
                .bind(now)
                .execute(&mut *conn)
                .await;
            }
            result.map(|_| ())
        })).await
    }

    pub async fn update_settings(
        &self, user_id: i64,
        update: crate::models::settings::SettingsUpdate,
        encryption_key: [u8; 32],
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            let mut tx = conn.begin().await?;

            let _ = sqlx::query(
                "INSERT OR IGNORE INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, provider_type, updated_at) VALUES (?, '', '', '', '', 8192, '', 0, 'openai', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut *tx)
            .await;

            sqlx::query(
                "UPDATE settings SET api_base_url = ?, model_name = ?, system_prompt = ?, context_limit = ?, post_history_instructions = ?, forbid_external_media = ?, display_name_overrides_persona = ?, provider_type = ?, \
                 summary_provider_type = ?, summary_api_base_url = ?, summary_model_name = ?, summary_context_limit = ?, \
                 embedding_source = ?, embedding_api_base_url = ?, embedding_model_name = ?, \
                 rag_top_k = ?, rag_score_threshold = ?, \
                 temperature = ?, top_p = ?, top_k = ?, frequency_penalty = ?, presence_penalty = ?, max_response_tokens = ?, reasoning_effort = ?, \
                 updated_at = ? WHERE user_id = ?",
            )
            .bind(&update.api_base_url)
            .bind(&update.model_name)
            .bind(&update.system_prompt)
            .bind(update.context_limit)
            .bind(&update.post_history_instructions)
            .bind(update.forbid_external_media)
            .bind(update.display_name_overrides_persona)
            .bind(&update.provider_type)
            .bind(&update.summary_provider_type)
            .bind(&update.summary_api_base_url)
            .bind(&update.summary_model_name)
            .bind(update.summary_context_limit)
            .bind(&update.embedding_source)
            .bind(&update.embedding_api_base_url)
            .bind(&update.embedding_model_name)
            .bind(update.rag_top_k)
            .bind(update.rag_score_threshold)
            .bind(update.temperature)
            .bind(update.top_p)
            .bind(update.top_k)
            .bind(update.frequency_penalty)
            .bind(update.presence_penalty)
            .bind(update.max_response_tokens)
            .bind(&update.reasoning_effort)
            .bind(now)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;

            if let Some(key) = update.api_key {
                let encrypted = crate::crypto::encrypt(&encryption_key, &key)
                    .map_err(|e| sqlx::Error::Configuration(format!("API key encryption failed: {e}").into()))?;
                sqlx::query("UPDATE settings SET api_key = ? WHERE user_id = ?")
                    .bind(&encrypted)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await?;
            }

            if let Some(key) = update.summary_api_key {
                let encrypted = crate::crypto::encrypt(&encryption_key, &key)
                    .map_err(|e| sqlx::Error::Configuration(format!("API key encryption failed: {e}").into()))?;
                sqlx::query("UPDATE settings SET summary_api_key = ? WHERE user_id = ?")
                    .bind(&encrypted)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await?;
            }

            if let Some(key) = update.embedding_api_key {
                let encrypted = crate::crypto::encrypt(&encryption_key, &key)
                    .map_err(|e| sqlx::Error::Configuration(format!("API key encryption failed: {e}").into()))?;
                sqlx::query("UPDATE settings SET embedding_api_key = ? WHERE user_id = ?")
                    .bind(&encrypted)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;
            Ok(())
        })).await
    }
}
