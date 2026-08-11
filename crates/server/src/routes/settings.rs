use serde::{Deserialize, Serialize};
use crate::models::settings::{SettingsExport, SettingsUpdate, SettingsView};
use crate::state::AppState;
use crate::error::ApiError;
use axum::extract::{Extension, Query, State};
use axum::Json;

pub async fn get(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<SettingsView>, ApiError> {
    Ok(Json(crate::models::settings::get_view(&state.db.read_pool, user_id).await?))
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(mut update): Json<SettingsUpdate>,
) -> Result<axum::http::StatusCode, ApiError> {
    update.api_key = update.api_key.filter(|key| !key.is_empty());
    update.summary_api_key = update.summary_api_key.filter(|key| !key.is_empty());
    update.embedding_api_key = update.embedding_api_key.filter(|key| !key.is_empty());

    // input length validation
    const MAX_SYSTEM_PROMPT: usize = 500_000;
    const MAX_TEXT_FIELD: usize = 100_000;
    if update.system_prompt.len() > MAX_SYSTEM_PROMPT {
        return Err(ApiError::bad_request("system_prompt exceeds maximum length"));
    }
    if update.api_base_url.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("api_base_url exceeds maximum length"));
    }
    if update.model_name.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("model_name exceeds maximum length"));
    }
    if update.post_history_instructions.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("post_history_instructions exceeds maximum length"));
    }
    if update.summary_api_base_url.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("summary_api_base_url exceeds maximum length"));
    }
    if update.summary_model_name.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("summary_model_name exceeds maximum length"));
    }
    if update.embedding_api_base_url.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("embedding_api_base_url exceeds maximum length"));
    }
    if update.embedding_model_name.len() > MAX_TEXT_FIELD {
        return Err(ApiError::bad_request("embedding_model_name exceeds maximum length"));
    }

    state
        .db
        .writer
        .update_settings(user_id, update, state.encryption_key)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to update settings");
            ApiError::internal("Failed to update settings")
        })?;
    Ok(axum::http::StatusCode::OK)
}

// everything in settings except API keys and identity fields, for backup/
// re-import on this install or another one
pub async fn export(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<SettingsExport>, ApiError> {
    Ok(Json(crate::models::settings::get_export(&state.db.read_pool, user_id).await?))
}

// applies an exported snapshot, never touches stored keys. dangling
// active_preset_id (from a different install) just gets skipped
pub async fn import(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(export): Json<SettingsExport>,
) -> Result<axum::http::StatusCode, ApiError> {
    let active_preset_id = export.active_preset_id.clone();
    state
        .db
        .writer
        .update_settings(user_id, export.into_update(), state.encryption_key)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to import settings");
            ApiError::internal("Failed to import settings")
        })?;

    match active_preset_id {
        Some(id) if crate::models::preset::get(&state.db.read_pool, user_id, &id).await?.is_some() => {
            let _ = state.db.writer.set_active_preset(user_id, Some(id)).await;
        }
        None => {
            let _ = state.db.writer.set_active_preset(user_id, None).await;
        }
        _ => {}
    }

    Ok(axum::http::StatusCode::OK)
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Serialize)]
pub struct ModelListItem {
    pub id: String,
}

#[derive(Deserialize)]
pub struct ListModelsParams {
    #[serde(default)]
    pub subscription_only: bool,
}

/// janky nanogpt-only hack: `.../api/v1` -> `.../api/subscription/v1`,
/// their docs say that only returns subscription models not the whole
/// pay-as-you-go mess. no-op if base url doesn't end in /v1, no clean way
/// to guess this path for anyone else
fn subscription_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    match trimmed.strip_suffix("/v1") {
        Some(prefix) => format!("{prefix}/subscription/v1/models"),
        None => format!("{trimmed}/models"),
    }
}

// hits the provider's /models endpoint server-side, only model ids reach the browser
pub async fn list_models(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Query(params): Query<ListModelsParams>,
) -> Result<Json<Vec<ModelListItem>>, ApiError> {
    let view = crate::models::settings::get_view(&state.db.read_pool, user_id).await?;
    if view.api_base_url.is_empty() {
        return Err(ApiError::bad_request("API base URL is not set"));
    }
    if !view.has_api_key {
        return Err(ApiError::bad_request("API key is not set"));
    }
    let api_key = crate::models::settings::get_decrypted_api_key(&state.db.read_pool, user_id, &state.encryption_key).await?;

    let url = if params.subscription_only {
        subscription_models_url(&view.api_base_url)
    } else {
        format!("{}/models", view.api_base_url.trim_end_matches('/'))
    };
    let mut request = state.http_client.get(url);
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "provider models list request failed");
            ApiError::new(502, format!("could not reach {}: {}", view.api_base_url, e))
        })?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(ApiError::new(502, format!("the provider returned {status}, check the API base URL and key are correct")));
    }

    let parsed: ModelsResponse = response
        .json()
        .await
        .map_err(|_| ApiError::new(502, "the provider's response wasn't in the expected format".to_string()))?;
    let mut ids: Vec<ModelListItem> = parsed.data.into_iter().map(|m| ModelListItem { id: m.id }).collect();
    ids.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(ids))
}
