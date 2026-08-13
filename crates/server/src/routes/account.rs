use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DeleteAccountDataRequest {
    pub username: String,
}

pub async fn export_all(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<crate::models::account::AccountExport>, ApiError> {
    Ok(Json(crate::models::account::export_all(&state.db.read_pool, user_id).await?))
}

pub async fn delete_all(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(body): Json<DeleteAccountDataRequest>,
) -> Result<StatusCode, ApiError> {
    let user = crate::models::user::find_by_id(&state.db.read_pool, user_id)
        .await?
        .ok_or_else(|| ApiError::internal("user not found"))?;
    if body.username != user.username {
        return Err(ApiError::forbidden("username does not match"));
    }

    let avatar_urls = state.db.writer.delete_all_account_content(user_id).await?;

    let upload_dir = crate::resolve_path("crates/server/uploads");
    for url in avatar_urls {
        let Some(filename) = url.strip_prefix("/uploads/") else { continue };
        let filename = filename.split('?').next().unwrap_or(filename);
        if filename.is_empty() || filename.contains('/') || filename.contains("..") {
            continue;
        }
        let path = upload_dir.join(filename);
        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %path.display(), "failed to remove avatar file during account data deletion");
            }
        }
    }

    Ok(StatusCode::OK)
}
