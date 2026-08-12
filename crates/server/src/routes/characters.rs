use crate::models::character::{
    AlternateGreeting, AlternateGreetingInput, Character, CharacterInput, Folder, FolderInput,
    Tag, TagInput,
};
use crate::state::AppState;
use axum::extract::{Extension, Multipart, Path, State};
use axum::Json;
use axum::http::StatusCode;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use crate::error::ApiError;

const MAX_NAME: usize = 256;
const MAX_TEXT_FIELD: usize = 100_000;
const MAX_SYSTEM_PROMPT: usize = 500_000;

fn validate_character_input(input: &CharacterInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if input.name.len() > MAX_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }

    for (field_name, value, max_len) in [
        ("description", input.description.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("personality", input.personality.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("scenario", input.scenario.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("first_message", input.first_message.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("sample_chat", input.sample_chat.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("system_prompt", input.system_prompt.as_deref().unwrap_or(""), MAX_SYSTEM_PROMPT),
        ("post_history_instructions", input.post_history_instructions.as_deref().unwrap_or(""), MAX_SYSTEM_PROMPT),
        ("prefill", input.prefill.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("insert_depth_prompt", input.insert_depth_prompt.as_deref().unwrap_or(""), MAX_TEXT_FIELD),
        ("persona", input.persona.as_deref().unwrap_or("{}"), MAX_TEXT_FIELD),
        ("extensions", input.extensions.as_deref().unwrap_or("{}"), MAX_TEXT_FIELD),
    ] {
        if value.len() > max_len {
            return Err(ApiError::bad_request(format!("{} too long (max {} characters)", field_name, max_len)));
        }
    }

    if let Some(url) = &input.avatar_url {
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("/uploads/") {
            return Err(ApiError::bad_request("avatar_url must be http(s) or /uploads/ path"));
        }
    }

    if let Some(folder_id) = &input.folder_id {
        if folder_id.len() > 64 {
            return Err(ApiError::bad_request("folder_id too long"));
        }
    }

    Ok(())
}

const MAX_GREETING: usize = 100_000;
const MAX_TAG_NAME: usize = 64;
const MAX_TAG_COLOR: usize = 7;
const MAX_FOLDER_NAME: usize = 256;

fn validate_greeting_input(input: &AlternateGreetingInput) -> Result<(), ApiError> {
    if input.greeting.trim().is_empty() {
        return Err(ApiError::bad_request("Greeting cannot be empty"));
    }
    if input.greeting.len() > MAX_GREETING {
        return Err(ApiError::bad_request("Greeting too long (max 100,000 characters)"));
    }
    Ok(())
}

fn validate_tag_input(input: &TagInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Tag name cannot be empty"));
    }
    if input.name.len() > MAX_TAG_NAME {
        return Err(ApiError::bad_request("Tag name too long (max 64 characters)"));
    }
    if let Some(color) = &input.color {
        if !color.starts_with('#') || color.len() != MAX_TAG_COLOR {
            return Err(ApiError::bad_request("Color must be a hex code like #RRGGBB"));
        }
    }
    Ok(())
}

fn validate_folder_input(input: &FolderInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Folder name cannot be empty"));
    }
    if input.name.len() > MAX_FOLDER_NAME {
        return Err(ApiError::bad_request("Folder name too long (max 256 characters)"));
    }
    if let Some(parent_id) = &input.parent_id {
        if parent_id.len() > 64 {
            return Err(ApiError::bad_request("parent_id too long"));
        }
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Character>>, ApiError> {
    Ok(Json(crate::models::character::list(&state.db.read_pool, user_id).await?))
}

pub async fn get_character(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Character>, ApiError> {
    crate::models::character::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Character not found"))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<CharacterInput>,
) -> Result<Json<Character>, ApiError> {
    validate_character_input(&input)?;
    state
        .db
        .writer
        .create_character(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create character");
            ApiError::internal("Failed to create character")
        })
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<CharacterInput>,
) -> Result<StatusCode, ApiError> {
    validate_character_input(&input)?;
    let updated = state
        .db
        .writer
        .update_character(user_id, id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update character");
            ApiError::internal("Failed to update character")
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
        .delete_character(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete character");
            ApiError::internal("Failed to delete character")
        })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn list_greetings(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
) -> Result<Json<Vec<AlternateGreeting>>, ApiError> {
    Ok(Json(
        crate::models::character::list_alternate_greetings(&state.db.read_pool, user_id, &character_id).await?,
    ))
}

pub async fn add_greeting(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    Json(input): Json<AlternateGreetingInput>,
) -> Result<Json<AlternateGreeting>, ApiError> {
    validate_greeting_input(&input)?;
    state
        .db
        .writer
        .create_alternate_greeting(user_id, character_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create greeting");
            ApiError::internal("Failed to create greeting")
        })
}

pub async fn update_greeting(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((character_id, greeting_id)): Path<(String, String)>,
    Json(input): Json<AlternateGreetingInput>,
) -> Result<StatusCode, ApiError> {
    validate_greeting_input(&input)?;
    let updated = state
        .db
        .writer
        .update_alternate_greeting(user_id, character_id, greeting_id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update greeting");
            ApiError::internal("Failed to update greeting")
        })?;
    Ok(if updated { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn delete_greeting(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((character_id, greeting_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .db
        .writer
        .delete_alternate_greeting(user_id, character_id, greeting_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete greeting");
            ApiError::internal("Failed to delete greeting")
        })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn list_tags(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Tag>>, ApiError> {
    Ok(Json(crate::models::character::list_tags(&state.db.read_pool, user_id).await?))
}

pub async fn create_tag(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<TagInput>,
) -> Result<Json<Tag>, ApiError> {
    state
        .db
        .writer
        .create_tag(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create tag");
            ApiError::internal("Failed to create tag")
        })
}

pub async fn delete_tag(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .db
        .writer
        .delete_tag(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete tag");
            ApiError::internal("Failed to delete tag")
        })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn list_character_tags(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(
        crate::models::character::list_character_tags(&state.db.read_pool, user_id, &character_id).await?,
    ))
}

pub async fn set_character_tags(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    Json(tag_ids): Json<Vec<String>>,
) -> Result<StatusCode, ApiError> {
    crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Character not found"))?;

    state
        .db
        .writer
        .set_character_tags(user_id, character_id, tag_ids)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set character tags");
            ApiError::internal("Failed to set character tags")
        })?;
    Ok(StatusCode::OK)
}

pub async fn list_folders(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Folder>>, ApiError> {
    Ok(Json(crate::models::character::list_folders(&state.db.read_pool, user_id).await?))
}

pub async fn create_folder(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<FolderInput>,
) -> Result<Json<Folder>, ApiError> {
    state
        .db
        .writer
        .create_folder(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create folder");
            ApiError::internal("Failed to create folder")
        })
}

pub async fn update_folder(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<FolderInput>,
) -> Result<StatusCode, ApiError> {
    let updated = state
        .db
        .writer
        .update_folder(user_id, id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update folder");
            ApiError::internal("Failed to update folder")
        })?;
    Ok(if updated { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

pub async fn delete_folder(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let deleted = state
        .db
        .writer
        .delete_folder(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete folder");
            ApiError::internal("Failed to delete folder")
        })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

fn uploads_dir() -> PathBuf {
    crate::auth::uploads_dir()
}

pub async fn upload_avatar(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // ownership check: verify the character belongs to this user
    let _character = crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to check character ownership for avatar upload");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        })?
        .ok_or((StatusCode::NOT_FOUND, "Character not found".to_string()))?;

    // validate avatar_url scheme to http, https, or /uploads/
    // are accepted (handled at the API layer via CharacterInput validation).

    // ensure the uploads directory exists
    let upload_dir = uploads_dir();
    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to create uploads dir: {e}")))?;

    // a card PNG (embedded portrait plus its JSON metadata chunk) routinely
    // lands well past 5MB on its own; character import re-uploads that same
    // original file as the avatar, so this has to comfortably exceed real
    // card sizes, not just a plain profile picture. stays under the 25MB
    // whole-request body limit (see routes::mAX_BODY_BYTES).
    const MAX_AVATAR_SIZE: usize = 20 * 1024 * 1024;

    // allowed extensions whitelist
    const ALLOWED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let Some(name) = field.name().map(|s| s.to_string()) else {
            continue;
        };

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "avatar".to_string());

        // determine extension from filename
        let ext = PathBuf::from(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        // validate extension against whitelist
        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            return Err((StatusCode::BAD_REQUEST, format!("File extension '.{ext}' is not allowed. Allowed: png, jpg, jpeg, gif, webp")));
        }

        let stored_name = format!("avatar_{character_id}.{ext}");
        let path = upload_dir.join(&stored_name);

        // chunk-by-chunk size check, not `field.bytes()` which would just
        // buffer the whole upload before we even get to check MAX_AVATAR_SIZE
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

        // the stored filename is stable per character (same character_id +
        // extension), so re-uploading a replacement overwrites the same
        // path on disk. a cache-busting query string is required or the
        // URL never changes, which means neither the browser's HTTP cache
        // nor Leptos re-rendering an <img> with an unchanged `src` will
        // ever pick up the new image.
        let version = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let url = format!("/uploads/{stored_name}?v={version}");

        state.db.writer.update_character_avatar(user_id, character_id.clone(), Some(url.clone()))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to save avatar: {e}")))?;

        return Ok(Json(serde_json::json!({ "url": url, "field": name })));
    }

    Err((StatusCode::BAD_REQUEST, "no file field found".to_string()))
}
