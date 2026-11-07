use crate::models::chat::Chat;
use crate::models::message::MessageTree;
use crate::provider::prompt::Role;
use crate::state::AppState;
use crate::error::ApiError;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
pub struct RecentQuery {
    pub limit: Option<i64>,
}

pub async fn list_recent(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Query(query): Query<RecentQuery>,
) -> Result<Json<Vec<crate::models::chat::RecentChat>>, ApiError> {
    let limit = query.limit.unwrap_or(8).clamp(1, 40);
    Ok(Json(
        crate::models::chat::list_recent(&state.db.read_pool, user_id, limit).await?,
    ))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    Json(input): Json<CreateChatInput>,
) -> Result<Json<Chat>, ApiError> {
    validate_chat_title(&input.title)?;
    crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Character not found"))?;
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
    /// drop each message's stored provider payload. the chat view reloads
    /// this after every reply, and those payloads are what make the body
    /// take seconds to arrive.
    pub brief: Option<bool>,
}

pub async fn get_tree(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<MessageTree>, ApiError> {
    let mut tree =
        crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?;
    if params.brief.unwrap_or(false) {
        crate::models::message::strip_raw_prompts(&mut tree);
    }
    Ok(Json(tree))
}

#[derive(Deserialize)]
pub struct BranchParams {
    pub brief: Option<bool>,
}

pub async fn get_active_branch(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(params): Query<BranchParams>,
) -> Result<Json<Vec<crate::models::message::Message>>, ApiError> {
    let mut messages = crate::models::message::active_branch_for_chat(
        &state.db.read_pool,
        user_id,
        &chat_id,
        Some(200),
    )
    .await?;
    if params.brief.unwrap_or(false) {
        for message in &mut messages {
            message.raw_prompt = None;
        }
    }
    Ok(Json(messages))
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

#[derive(Serialize)]
pub struct MessagePrompt {
    pub raw_prompt: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub context_limit: Option<i64>,
}

/// one message's stored provider payload, loaded only when the prompt panel
/// is opened. the history reload leaves this out.
pub async fn get_message_prompt(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MessagePrompt>, ApiError> {
    let msg = sqlx::query_as::<_, crate::models::message::Message>(
        "SELECT * FROM messages WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(user_id)
    .fetch_optional(&state.db.read_pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "failed to load message prompt");
        ApiError::internal("Failed to load message")
    })?;
    let msg = msg.ok_or_else(|| ApiError::not_found("Message not found"))?;
    Ok(Json(MessagePrompt {
        raw_prompt: msg.raw_prompt,
        prompt_tokens: msg.prompt_tokens,
        context_limit: msg.context_limit,
    }))
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
) -> Result<Json<std::collections::BTreeMap<String, crate::models::message::MessageNode>>, ApiError> {
    let nodes = crate::models::message::tree_from_message(
        &state.db.read_pool,
        user_id,
        &chat_id,
        &params.from,
        params.depth,
    )
    .await?;
    // sorted, so every request for the same branch returns the same bytes
    Ok(Json(nodes.into_iter().collect()))
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
    change_member(&state, user_id, chat_id, input.character_id, true).await
}

pub async fn remove_member(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path((chat_id, character_id)): Path<(String, String)>,
) -> Result<Json<Chat>, ApiError> {
    change_member(&state, user_id, chat_id, character_id, false).await
}

async fn change_member(
    state: &AppState, user_id: i64, chat_id: String, character_id: String, add: bool,
) -> Result<Json<Chat>, ApiError> {
    state.db.writer.change_chat_member(user_id, chat_id.clone(), character_id, add)
        .await.map_err(|error| match error {
            sqlx::Error::RowNotFound => ApiError::not_found("Chat or character not found"),
            sqlx::Error::Protocol(message) if add => ApiError::conflict(message),
            sqlx::Error::Protocol(message) => ApiError::unprocessable(message),
            other => ApiError::from_sqlx(other),
        })?;
    crate::models::chat::get(&state.db.read_pool, user_id, &chat_id).await?
        .map(Json).ok_or_else(|| ApiError::not_found("Chat not found"))
}
