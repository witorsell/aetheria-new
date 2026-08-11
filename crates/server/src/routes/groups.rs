use crate::error::ApiError;
use crate::models::group::{Group, GroupInput, GroupWithMembers};
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    Json,
};
use serde::Deserialize;

const MAX_GROUP_NAME: usize = 256;
const MAX_AVATAR_URL: usize = 2048;

fn validate_group_input(input: &GroupInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if input.name.len() > MAX_GROUP_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    if let Some(url) = &input.avatar_url {
        if !url.is_empty() && url.len() > MAX_AVATAR_URL {
            return Err(ApiError::bad_request("avatar_url too long"));
        }
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("/uploads/") {
            return Err(ApiError::bad_request("avatar_url must be http(s) or /uploads/ path"));
        }
    }
    if let Some(strategy) = &input.activation_strategy {
        if strategy.len() > 64 {
            return Err(ApiError::bad_request("activation_strategy too long"));
        }
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<Group>>, ApiError> {
    Ok(Json(
        crate::models::group::list(&state.db.read_pool, user_id).await?,
    ))
}

pub async fn get_group(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<GroupWithMembers>, ApiError> {
    let group = crate::models::group::get(&state.db.read_pool, user_id, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Group not found"))?;
    let members = crate::models::group::list_members(&state.db.read_pool, &id).await?;
    Ok(Json(GroupWithMembers { group, members }))
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<GroupInput>,
) -> Result<Json<Group>, ApiError> {
    validate_group_input(&input)?;
    state
        .db
        .writer
        .create_group(user_id, input)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create group");
            ApiError::internal("Failed to create group")
        })
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<GroupInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate_group_input(&input)?;
    let success = state
        .db
        .writer
        .update_group(user_id, id, input)
        .await?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

pub async fn delete_group(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let success = state
        .db
        .writer
        .delete_group(user_id, id)
        .await?;
    Ok(if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

#[derive(Deserialize)]
pub struct MemberInput {
    pub character_id: String,
    pub disabled: bool,
}

#[derive(Deserialize)]
pub struct SetMembersRequest {
    pub members: Vec<MemberInput>,
}

pub async fn set_members(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetMembersRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    // ownership check: set_group_members doesn't filter group_members writes
    // by user_id, so this route has to confirm the group itself belongs to
    // this user before touching its membership at all.
    crate::models::group::get(&state.db.read_pool, user_id, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Group not found"))?;

    // the group_members.character_id FK isn't scoped to a user, so a
    // character belonging to someone else would otherwise slot right in.
    // confirm every requested character_id actually belongs to this user
    // before handoff the list to the writer.
    let requested_ids: std::collections::HashSet<&str> =
        req.members.iter().map(|m| m.character_id.as_str()).collect();
    if !requested_ids.is_empty() {
        let placeholders = requested_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT COUNT(*) FROM characters WHERE user_id = ? AND id IN ({placeholders})"
        );
        let mut q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query)).bind(user_id);
        for character_id in &requested_ids {
            q = q.bind(*character_id);
        }
        let owned_count: i64 = q
            .fetch_one(&state.db.read_pool)
            .await?;
        if owned_count as usize != requested_ids.len() {
            return Err(ApiError::not_found("One or more characters were not found"));
        }
    }

    let members = req.members.into_iter().map(|m| (m.character_id, m.disabled)).collect();
    state
        .db
        .writer
        .set_group_members(user_id, id, members)
        .await?;
    Ok(axum::http::StatusCode::OK)
}

pub async fn create_chat(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<crate::routes::chats::CreateChatInput>,
) -> Result<Json<crate::models::chat::Chat>, ApiError> {
    crate::models::group::get(&state.db.read_pool, user_id, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("Group not found"))?;

    state
        .db
        .writer
        .create_chat(user_id, None, Some(id), input.title)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create chat");
            ApiError::internal("Failed to create chat")
        })
}
