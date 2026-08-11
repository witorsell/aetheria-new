use crate::state::AppState;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::PrivateCookieJar;
use sqlx::Row;

pub async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let jar = PrivateCookieJar::from_headers(request.headers(), state.cookie_key.clone());
    if let Some(cookie) = jar.get("session") {
        let session_id = cookie.value().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        // sessions must be within both their absolute expiry and idle-timeout window
        let idle_cutoff = now - (state.session_idle_timeout.as_millis() as i64);

        let row = sqlx::query(
            "SELECT user_id, last_active_at FROM sessions WHERE id = ? AND expires_at > ? AND (last_active_at IS NULL OR last_active_at > ?)",
        )
        .bind(&session_id)
        .bind(now)
        .bind(idle_cutoff)
        .fetch_optional(&state.db.read_pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to look up session during auth");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Some(row) = row {
            let user_id: i64 = row.get("user_id");
            // push the idle-timeout window forward on each request via writer thread
            let _ = state
                .db
                .writer
                .update_session_activity(&session_id, now)
                .await;
            let mut request = request;
            request.extensions_mut().insert(user_id);
            return Ok(next.run(request).await);
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}
