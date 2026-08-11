use super::*;

impl Writer {
    pub async fn create_session(&self, user_id: i64, id: String, expires_at: i64) -> sqlx::Result<()> {
        let now = chrono_now_millis();
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "INSERT INTO sessions (user_id, id, expires_at, last_active_at, issued_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(id)
            .bind(expires_at)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| ())
        })).await
    }

    /// updates `last_active_at` so the idle-timeout window moves forward on each request
    pub async fn update_session_activity(&self, session_id: &str, now: i64) -> sqlx::Result<()> {
        let session_id = session_id.to_string();
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE sessions SET last_active_at = ? WHERE id = ?")
                .bind(now)
                .bind(session_id)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        })).await
    }

    /// removes sessions whose absolute expiry or idle timeout has elapsed
    pub async fn purge_expired_sessions(&self, now: i64, idle_cutoff: i64) -> sqlx::Result<u64> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "DELETE FROM sessions WHERE expires_at <= ? OR last_active_at <= ?",
            )
            .bind(now)
            .bind(idle_cutoff)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected())
        })).await
    }

    pub async fn delete_session(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "DELETE FROM sessions WHERE id = ? AND user_id = ?",
            )
            .bind(id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|res| res.rows_affected() > 0)
        })).await
    }
}
