use super::*;
use crate::models::theme::{Theme, ThemeTokens};

impl Writer {
    pub async fn create_theme(&self, user_id: i64, name: String, tokens: ThemeTokens) -> sqlx::Result<Theme> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let token_json = serde_json::to_string(&tokens).unwrap_or_else(|_| "{}".to_string());
            sqlx::query(
                "INSERT INTO themes (id, user_id, name, token_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(user_id)
            .bind(&name)
            .bind(&token_json)
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| Theme { id, user_id, name, tokens, created_at: now, updated_at: now })
        })).await
    }

    pub async fn update_theme(&self, user_id: i64, id: String, name: String, tokens: ThemeTokens) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            let token_json = serde_json::to_string(&tokens).unwrap_or_else(|_| "{}".to_string());
            sqlx::query("UPDATE themes SET name = ?, token_json = ?, updated_at = ? WHERE id = ? AND user_id = ?")
                .bind(&name)
                .bind(&token_json)
                .bind(now)
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn delete_theme(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM themes WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn set_active_theme(&self, user_id: i64, theme_id: String) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE settings SET active_theme_id = ? WHERE user_id = ?")
                .bind(&theme_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|_| ())
        })).await
    }

    pub async fn get_active_theme_id(&self, user_id: i64) -> sqlx::Result<String> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query_scalar::<_, String>("SELECT active_theme_id FROM settings WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await
                .map(|opt| opt.unwrap_or_else(|| "default".to_string()))
        })).await
    }
}
