use super::*;

impl Writer {
    pub async fn create_user(&self, username: String, password_hash: String) -> sqlx::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // both inserts must land in the same dispatch: the writer actor only
        // guarantees one job runs at a time, not that a caller's two separate
        // dispatch() calls run back-to-back with nothing interleaved between
        // them. a concurrent create_user's own INSERT landing between these
        // two used to point the settings row at (SELECT MAX(id) FROM users) -
        // whichever user was newest by the time THIS query ran, not
        // necessarily the one just created here.
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            sqlx::query(
                "INSERT INTO users (id, username, password_hash, session_secret)
                 VALUES ((SELECT COALESCE(MAX(id), 0) + 1 FROM users), ?, ?, '')",
            )
            .bind(&username)
            .bind(&password_hash)
            .execute(&mut *tx)
            .await?;

            let user_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
                .fetch_one(&mut *tx)
                .await?;

            sqlx::query(
                "INSERT INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, updated_at)
                 VALUES (?, '', '', '', '', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(())
        })).await
    }

    pub async fn upsert_user(&self, username: String, password_hash: String) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            // never overwrite a real existing user even if env creds change.
            // but migration 0011's placeholder id=1/'admin'/empty-hash row
            // (seeded on every fresh database for pre-existing single-user
            // installs' FK needs) isn't a real bootstrapped user, so the first
            // real boot claims it in place instead of being blocked by it.
            let placeholder_id: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM users WHERE id = 1 AND username = 'admin' AND password_hash = ''",
            )
            .fetch_optional(&mut *conn)
            .await?;

            if let Some(id) = placeholder_id {
                return sqlx::query("UPDATE users SET username = ?, password_hash = ? WHERE id = ?")
                    .bind(&username)
                    .bind(&password_hash)
                    .bind(id)
                    .execute(&mut *conn)
                    .await
                    .map(|_| ());
            }

            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                .fetch_one(&mut *conn)
                .await?;
            if count > 0 {
                return Ok(());
            }
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, session_secret) VALUES (1, ?, ?, '')",
            )
            .bind(&username)
            .bind(&password_hash)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    pub async fn update_user(
        &self, user_id: i64,
        display_name: Option<String>,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE users SET display_name = ? WHERE id = ?",
            )
            .bind(display_name)
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
