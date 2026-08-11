use crate::models::chat::Chat;
use crate::models::message::MessageTree;
use crate::provider::prompt::Role;
use crate::state::AppState;
use crate::error::ApiError;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;

const MAX_CHAT_TITLE: usize = 256;
const MAX_MESSAGE_CONTENT: usize = 100_000;

fn validate_chat_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::bad_request("Title cannot be empty"));
    }
    if title.len() > MAX_CHAT_TITLE {
        return Err(ApiError::bad_request("Title too long (max 256 characters)"));
    }
    Ok(())
}

fn validate_message_content(content: &str) -> Result<(), ApiError> {
    if content.len() > MAX_MESSAGE_CONTENT {
        return Err(ApiError::bad_request("Message too long (max 100,000 characters)"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateChatInput {
    pub title: String,
}

pub async fn get_chat(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Result<Json<Chat>, ApiError> {
    crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Chat not found"))
}

pub async fn list_for_character(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
) -> Result<Json<Vec<Chat>>, ApiError> {
    Ok(Json(
        crate::models::chat::list_for_character(&state.db.read_pool, user_id, &character_id).await?,
    ))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    Json(input): Json<CreateChatInput>,
) -> Result<Json<Chat>, ApiError> {
    validate_chat_title(&input.title)?;
    let chat = state
        .db
        .writer
        .create_chat(user_id, Some(character_id.clone()), None, input.title)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create chat");
            ApiError::internal("Failed to create chat")
        })?;

    if let Some(character) = crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await?
    {
        if !character.first_message.is_empty() {
            state
                .db
                .writer
                .create_message(user_id, chat.id.clone(), None, Role::Assistant.to_string(), character.first_message)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to create initial message");
                    ApiError::internal("Failed to create initial message")
                })?;
        }
    }

    Ok(Json(chat))
}

#[derive(Deserialize)]
pub struct ListMessagesParams {
    pub before: Option<String>,
    pub limit: Option<i64>,
}

pub async fn get_tree(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(_params): Query<ListMessagesParams>,
) -> Result<Json<MessageTree>, ApiError> {
    Ok(Json(
        crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?,
    ))
}

pub async fn get_active_branch(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Result<Json<Vec<crate::models::message::Message>>, ApiError> {
    Ok(Json(
        crate::models::message::active_branch_for_chat(&state.db.read_pool, user_id, &chat_id, Some(200)).await?,
    ))
}

#[derive(Deserialize)]
pub struct DeleteMessageParams {
    pub hard: Option<bool>,
}

pub async fn delete_message(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DeleteMessageParams>,
) -> Result<StatusCode, ApiError> {
    let deleted = if params.hard.unwrap_or(false) {
        state.db.writer.hard_delete_message(user_id, id).await
    } else {
        state.db.writer.soft_delete_message(user_id, id).await
    };
    let deleted = deleted.map_err(|e| {
        tracing::error!(error = %e, "failed to delete message");
        ApiError::internal("Failed to delete message")
    })?;
    Ok(if deleted { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

#[derive(Deserialize)]
pub struct EditMessageInput {
    pub content: String,
}

pub async fn edit_message(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<EditMessageInput>,
) -> Result<StatusCode, ApiError> {
    validate_message_content(&input.content)?;
    let updated = state
        .db
        .writer
        .update_message_content(user_id, id, input.content)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to edit message");
            ApiError::internal("Failed to edit message")
        })?;
    Ok(if updated { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

#[derive(Deserialize)]
pub struct VisibilityInput {
    pub visible: bool,
}

pub async fn set_message_visibility(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<VisibilityInput>,
) -> Result<StatusCode, ApiError> {
    let updated = state
        .db
        .writer
        .set_message_visibility(user_id, id, input.visible)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update message visibility");
            ApiError::internal("Failed to update message visibility")
        })?;
    Ok(if updated { StatusCode::OK } else { StatusCode::NOT_FOUND })
}

#[derive(Deserialize)]
pub struct TreeFromParams {
    pub from: String,
    #[serde(default = "default_tree_depth")]
    pub depth: usize,
}

fn default_tree_depth() -> usize {
    50
}

/// fetch a subtree of messages rooted at `from`, up to `depth` generations.
/// for lazy-loading branches in the message tree.
pub async fn tree_from_message(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(params): Query<TreeFromParams>,
) -> Result<Json<HashMap<String, crate::models::message::MessageNode>>, ApiError> {
    use std::collections::HashMap;
    let nodes = crate::models::message::tree_from_message(
        &state.db.read_pool,
        user_id,
        &chat_id,
        &params.from,
        params.depth,
    )
    .await?;
    Ok(Json(nodes))
}

#[derive(Deserialize)]
pub struct AddChatMemberInput {
    pub character_id: String,
}

pub async fn add_member(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Json(input): Json<AddChatMemberInput>,
) -> Result<Json<Chat>, ApiError> {
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Chat not found"))?;

    let character = crate::models::character::get(&state.db.read_pool, user_id, &input.character_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Character not found"))?;

    if let Some(group_id) = chat.group_id.clone() {
        let mut members: Vec<(String, bool)> = crate::models::group::list_members(&state.db.read_pool, &group_id)
            .await?
            .into_iter()
            .map(|m| (m.character_id, m.disabled))
            .collect();
        if members.iter().any(|(id, _)| id == &input.character_id) {
            return Err(ApiError::conflict("Character is already a member of this group"));
        }
        members.push((input.character_id, false));
        state.db.writer.set_group_members(user_id, group_id, members)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to set group members");
                ApiError::internal("Failed to set group members")
            })?;
    } else {
        let existing_character_id = chat.character_id.clone().ok_or_else(|| {
            ApiError::unprocessable("Chat has no character and no group")
        })?;
        if existing_character_id == input.character_id {
            return Err(ApiError::conflict("Character is already in this chat"));
        }
        let existing_character = crate::models::character::get(&state.db.read_pool, user_id, &existing_character_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Existing character not found"))?;

        state.db.writer.convert_chat_to_group_with_new_member(
            user_id,
            chat_id.clone(),
            existing_character_id,
            input.character_id,
            format!("{} & {}", existing_character.name, character.name),
        ).await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to convert chat to group");
            ApiError::internal("Failed to convert chat to group")
        })?;
    }

    let updated = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Chat not found after update"))?;
    Ok(Json(updated))
}

pub async fn remove_member(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((chat_id, character_id)): Path<(String, String)>,
) -> Result<Json<Chat>, ApiError> {
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Chat not found"))?;
    let group_id = chat.group_id.clone().ok_or_else(|| {
        ApiError::unprocessable("Chat has no group")
    })?;

    let members = crate::models::group::list_members(&state.db.read_pool, &group_id).await?;
    let remaining: Vec<(String, bool)> = members
        .into_iter()
        .filter(|m| m.character_id != character_id)
        .map(|m| (m.character_id, m.disabled))
        .collect();

    if remaining.is_empty() {
        return Err(ApiError::unprocessable("Cannot remove the last member from a group chat"));
    }

    if remaining.len() == 1 {
        let last_character_id = remaining[0].0.clone();
        state.db.writer.convert_chat_to_direct(user_id, chat_id.clone(), last_character_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to convert chat to direct");
                ApiError::internal("Failed to convert chat to direct")
            })?;
        state.db.writer.delete_group(user_id, group_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to delete group");
                ApiError::internal("Failed to delete group")
            })?;
    } else {
        state.db.writer.set_group_members(user_id, group_id, remaining)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to set group members");
                ApiError::internal("Failed to set group members")
            })?;
    }

    let updated = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Chat not found after update"))?;
    Ok(Json(updated))
}
