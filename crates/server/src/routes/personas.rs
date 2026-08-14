use crate::models::persona::{Persona, PersonaInput};
use crate::state::AppState;
use axum::extract::{Extension, Multipart, Path, State};
use axum::http::StatusCode;
use axum::Json;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use crate::error::ApiError;

const MAX_NAME: usize = 256;
const MAX_DESCRIPTION: usize = 100_000;

fn validate_persona_input(input: &PersonaInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if input.name.len() > MAX_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    if input.description.as_deref().unwrap_or("").len() > MAX_DESCRIPTION {
        return Err(ApiError::bad_request("Description too long (max 100,000 characters)"));
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Persona>>, ApiError> {
    Ok(Json(crate::models::persona::list(&state.db.read_pool, user_id).await?))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<PersonaInput>,
) -> Result<Json<Persona>, ApiError> {
    validate_persona_input(&input)?;
    state
        .db
        .writer
        .create_persona(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create persona");
            ApiError::internal("Failed to create persona")
        })
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<PersonaInput>,
) -> Result<StatusCode, ApiError> {
    validate_persona_input(&input)?;
    let updated = state
        .db
        .writer
        .update_persona(user_id, id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update persona");
            ApiError::internal("Failed to update persona")
        })?;
    Ok(if updated { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn delete(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .db
        .writer
        .delete_persona(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete persona");
            ApiError::internal("Failed to delete persona")
        })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

#[derive(serde::Deserialize)]
pub struct SetActivePersonaRequest {
    pub persona_id: Option<String>,
}

pub async fn set_active(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(req): Json<SetActivePersonaRequest>,
) -> Result<StatusCode, ApiError> {
    let ok = state
        .db
        .writer
        .set_active_persona(user_id, req.persona_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set active persona");
            ApiError::internal("Failed to set active persona")
        })?;
    Ok(if ok { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

fn uploads_dir() -> PathBuf {
    crate::auth::uploads_dir()
}

pub async fn upload_avatar(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(persona_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let _persona = crate::models::persona::get(&state.db.read_pool, user_id, &persona_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to check persona ownership for avatar upload");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "Persona not found".to_string()))?;

    let upload_dir = uploads_dir();
    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to create uploads dir: {e}")))?;

    const MAX_AVATAR_SIZE: usize = 20 * 1024 * 1024;
    const ALLOWED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let Some(name) = field.name().map(|s| s.to_string()) else {
            continue;
        };

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "avatar".to_string());

        let ext = PathBuf::from(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            return Err((StatusCode::BAD_REQUEST, format!("File extension '.{ext}' is not allowed. Allowed: png, jpg, jpeg, gif, webp")));
        }

        let stored_name = format!("persona_{persona_id}.{ext}");
        let path = upload_dir.join(&stored_name);

        let mut data = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("read failed: {e}")))?
        {
            if data.len() + chunk.len() > MAX_AVATAR_SIZE {
                return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("File too large. Maximum size is {} bytes", MAX_AVATAR_SIZE)));
            }
            data.extend_from_slice(&chunk);
        }

        let mut file = fs::File::create(&path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("create file failed: {e}")))?;
        file.write_all(&data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")))?;

        let version = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let url = format!("/uploads/{stored_name}?v={version}");

        state.db.writer.update_persona_avatar(user_id, persona_id.clone(), Some(url.clone()))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db update failed: {e}")))?;

        let _ = name;
        return Ok(Json(serde_json::json!({ "avatar_url": url })));
    }

    Err((StatusCode::BAD_REQUEST, "No file field found".to_string()))
}
