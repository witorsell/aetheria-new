use super::*;

impl Writer {
    pub async fn create_user(&self, username: String, password_hash: String) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, session_secret)
                 VALUES ((SELECT COALESCE(MAX(id), 0) + 1 FROM users), ?, ?, '')",
            )
            .bind(username)
            .bind(password_hash)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, updated_at)
                 VALUES ((SELECT MAX(id) FROM users), '', '', '', '', ?)",
            )
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn upsert_user(&self, username: String, password_hash: String) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, session_secret) VALUES (1, ?, ?, '')
                 ON CONFLICT(id) DO UPDATE SET username = excluded.username, password_hash = excluded.password_hash",
            )
            .bind(username)
            .bind(password_hash)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn update_user(
        &self, user_id: i64,
        display_name: Option<String>,
        persona: Option<String>,
        use_persona: bool,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE users SET display_name = ?, persona = ?, use_persona = ? WHERE id = ?",
            )
            .bind(display_name)
            .bind(persona)
            .bind(use_persona)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn update_user_avatar(&self, user_id: i64, avatar_url: Option<String>) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE users SET avatar_url = ? WHERE id = ?")
                .bind(avatar_url)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        })).await
    }
}
