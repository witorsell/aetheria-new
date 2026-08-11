use axum::{
    extract::{Extension, Path, State},
    Json,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::models::lorebook::{
    CreateLorebookEntryInput, CreateLorebookInput, Lorebook, LorebookEntry,
};
use crate::state::AppState;

const MAX_LOREBOOK_NAME: usize = 256;
const MAX_LOREBOOK_DESCRIPTION: usize = 100_000;
const MAX_ENTRY_NAME: usize = 256;
const MAX_ENTRY_CONTENT: usize = 100_000;
const MAX_KEYWORDS: usize = 10_000;

fn validate_lorebook_input(input: &CreateLorebookInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if input.name.len() > MAX_LOREBOOK_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    if let Some(desc) = &input.description {
        if desc.len() > MAX_LOREBOOK_DESCRIPTION {
            return Err(ApiError::bad_request("Description too long (max 100,000 characters)"));
        }
    }
    if let Some(ext) = &input.extensions {
        if ext.len() > MAX_LOREBOOK_DESCRIPTION {
            return Err(ApiError::bad_request("extensions too long"));
        }
    }
    Ok(())
}

fn validate_lorebook_entry_input(input: &CreateLorebookEntryInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Entry name cannot be empty"));
    }
    if input.name.len() > MAX_ENTRY_NAME {
        return Err(ApiError::bad_request("Entry name too long (max 256 characters)"));
    }
    if let Some(keywords) = &input.keywords {
        if keywords.len() > MAX_KEYWORDS {
            return Err(ApiError::bad_request("Keywords too long"));
        }
    }
    if let Some(secondary) = &input.secondary_keys {
        if secondary.len() > MAX_KEYWORDS {
            return Err(ApiError::bad_request("Secondary keys too long"));
        }
    }
    if let Some(comment) = &input.comment {
        if comment.len() > MAX_ENTRY_CONTENT {
            return Err(ApiError::bad_request("Comment too long"));
        }
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Lorebook>>, ApiError> {
    Ok(Json(
        crate::models::lorebook::list(&state.db.read_pool, user_id).await?,
    ))
}

pub async fn get_lorebook(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Lorebook>, ApiError> {
    crate::models::lorebook::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Lorebook not found"))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<CreateLorebookInput>,
) -> Result<Json<Lorebook>, ApiError> {
    validate_lorebook_input(&input)?;
    state
        .db
        .writer
        .create_lorebook(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create lorebook");
            ApiError::internal("Failed to create lorebook")
        })
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<CreateLorebookInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_lorebook_input(&input)?;
    let success = state
        .db
        .writer
        .update_lorebook(user_id, id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update lorebook");
            ApiError::internal("Failed to update lorebook")
        })?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

pub async fn delete_lorebook(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let success = state
        .db
        .writer
        .delete_lorebook(user_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete lorebook");
            ApiError::internal("Failed to delete lorebook")
        })?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

pub async fn list_entries(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<LorebookEntry>>, ApiError> {
    Ok(Json(
        crate::models::lorebook::list_entries(&state.db.read_pool, user_id, &id).await?,
    ))
}

pub async fn get_entry(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((_lid, id)): Path<(String, String)>,
) -> Result<Json<LorebookEntry>, ApiError> {
    crate::models::lorebook::get_entry(&state.db.read_pool, user_id, &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Lorebook entry not found"))
}

pub async fn create_entry(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(lorebook_id): Path<String>,
    Json(mut input): Json<CreateLorebookEntryInput>,
) -> Result<Json<LorebookEntry>, ApiError> {
    validate_lorebook_entry_input(&input)?;
    input.lorebook_id = lorebook_id;
    state
        .db
        .writer
        .create_lorebook_entry(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create lorebook entry");
            ApiError::internal("Failed to create lorebook entry")
        })
}

pub async fn update_entry(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((lorebook_id, entry_id)): Path<(String, String)>,
    Json(mut input): Json<CreateLorebookEntryInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_lorebook_entry_input(&input)?;
    input.lorebook_id = lorebook_id;
    let success = state
        .db
        .writer
        .update_lorebook_entry(user_id, entry_id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update lorebook entry");
            ApiError::internal("Failed to update lorebook entry")
        })?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

pub async fn delete_entry(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((_lorebook_id, entry_id)): Path<(String, String)>,
) -> Result<axum::http::StatusCode, ApiError> {
    let success = state
        .db
        .writer
        .delete_lorebook_entry(user_id, entry_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to delete lorebook entry");
            ApiError::internal("Failed to delete lorebook entry")
        })?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

#[derive(Deserialize)]
pub struct SetLorebooksRequest {
    pub lorebook_ids: Vec<String>,
}

pub async fn set_character_lorebooks(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetLorebooksRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .db
        .writer
        .set_character_lorebooks(user_id, id, req.lorebook_ids)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set character lorebooks");
            ApiError::internal("Failed to set character lorebooks")
        })?;
    Ok(axum::http::StatusCode::OK)
}

pub async fn set_chat_lorebooks(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetLorebooksRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    state
        .db
        .writer
        .set_chat_lorebooks(user_id, id, req.lorebook_ids)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to set chat lorebooks");
            ApiError::internal("Failed to set chat lorebooks")
        })?;
    Ok(axum::http::StatusCode::OK)
}

pub async fn get_character_lorebooks(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(
        crate::models::lorebook::list_character_lorebooks(&state.db.read_pool, user_id, &id)
            .await?,
    ))
}

pub async fn get_chat_lorebooks(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, ApiError> {
    let chat_row = sqlx::query_as::<_, (String, bool)>(
        "SELECT character_id, lorebooks_customized FROM chats WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&state.db.read_pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Chat not found"))?;

    let (character_id, is_customized) = chat_row;

    let ids = if is_customized {
        crate::models::lorebook::list_chat_lorebooks(&state.db.read_pool, user_id, &id).await?
    } else {
        crate::models::lorebook::list_character_lorebooks(
            &state.db.read_pool,
            user_id,
            &character_id,
        )
        .await?
    };
    Ok(Json(ids))
}
