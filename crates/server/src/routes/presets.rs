use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Extension, Multipart, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

const MAX_PRESET_NAME: usize = 256;
const MAX_PROMPTS: usize = 500;
const MAX_PROMPT_CONTENT: usize = 100_000;
const MAX_PROMPT_ORDER: usize = 500;

fn validate_preset_name(name: &str) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if name.len() > MAX_PRESET_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    Ok(())
}

fn validate_preset_prompts(prompts: &[crate::models::preset::PresetPrompt]) -> Result<(), ApiError> {
    if prompts.len() > MAX_PROMPTS {
        return Err(ApiError::bad_request("Too many prompts (max 500)"));
    }
    for prompt in prompts {
        if prompt.content.len() > MAX_PROMPT_CONTENT {
            return Err(ApiError::bad_request("Prompt content too long (max 100,000 characters)"));
        }
        if prompt.name.len() > 256 {
            return Err(ApiError::bad_request("Prompt name too long (max 256 characters)"));
        }
        if prompt.identifier.len() > 128 {
            return Err(ApiError::bad_request("Prompt identifier too long (max 128 characters)"));
        }
    }
    Ok(())
}

fn validate_prompt_order(order: &[crate::models::preset::PresetOrderEntry]) -> Result<(), ApiError> {
    if order.len() > MAX_PROMPT_ORDER {
        return Err(ApiError::bad_request("Prompt order too long (max 500)"));
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::preset::Preset>>, ApiError> {
    Ok(Json(
        crate::models::preset::list(&state.db.read_pool, user_id).await?,
    ))
}

pub async fn get_preset(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::models::preset::Preset>, ApiError> {
    crate::models::preset::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Preset not found"))
}

#[derive(serde::Deserialize)]
pub struct UpdatePresetOrderInput {
    pub prompt_order: Vec<crate::models::preset::PresetOrderEntry>,
}

pub async fn update_order(
    Extension(user_id): Extension<i64>,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(input): Json<UpdatePresetOrderInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    let updated = state.db.writer.update_preset_order(user_id, id, input.prompt_order).await?;
    Ok(if updated {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

/// imports a SillyTavern completion preset export. only `prompts` and the
/// first `prompt_order` list's `order` are kept, everything else in the
/// export (sampling params, macro templates, etc.) aetheria doesn't have a
/// home for yet and is dropped.
pub async fn import(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes = None;
    let mut filename = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
        };
        if field.name() == Some("file") {
            filename = field.file_name().map(|s| s.to_string());
            match field.bytes().await {
                Ok(data) => file_bytes = Some(data),
                Err(e) => return (axum::http::StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
            }
        }
    }
    let bytes = match file_bytes {
        Some(b) => b,
        None => return (axum::http::StatusCode::BAD_REQUEST, "No file uploaded").into_response(),
    };
    let raw: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };

    let prompts: Vec<crate::models::preset::PresetPrompt> = match raw.get("prompts").cloned() {
        Some(v) => match serde_json::from_value(v) {
            Ok(p) => p,
            Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Could not parse the preset's prompts array").into_response(),
        },
        None => return (axum::http::StatusCode::BAD_REQUEST, "Missing a 'prompts' array, this doesn't look like a SillyTavern completion preset").into_response(),
    };

    // two shapes are accepted here: aetheria's own export (a flat array of
    // `{identifier, enabled}`) tried first, falling back to SillyTavern's
    // own nested `[{character_id, order: [...]}]` shape if that doesn't
    // parse, so a preset round-trips through export/import either way.
    let prompt_order: Vec<crate::models::preset::PresetOrderEntry> = raw
        .get("prompt_order")
        .and_then(|v| {
            serde_json::from_value::<Vec<crate::models::preset::PresetOrderEntry>>(v.clone())
                .ok()
                .or_else(|| {
                    v.as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|first| first.get("order").cloned())
                        .and_then(|order_val| serde_json::from_value(order_val).ok())
                })
        })
        .unwrap_or_default();

    let name = filename
        .map(|f| f.trim_end_matches(".json").to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Imported Preset".to_string());

    if let Err(e) = validate_preset_name(&name) {
        return (axum::http::StatusCode::BAD_REQUEST, format!("{:?}", e)).into_response();
    }
    if let Err(e) = validate_preset_prompts(&prompts) {
        return (axum::http::StatusCode::BAD_REQUEST, format!("{:?}", e)).into_response();
    }
    if let Err(e) = validate_prompt_order(&prompt_order) {
        return (axum::http::StatusCode::BAD_REQUEST, format!("{:?}", e)).into_response();
    }

    match state.db.writer.create_preset(user_id, name, prompts, prompt_order).await {
        Ok(preset) => Json(preset).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to save preset").into_response(),
    }
}

pub async fn export_preset(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::models::preset::PresetExport>, ApiError> {
    crate::models::preset::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(|p| Json(p.into()))
        .ok_or_else(|| ApiError::not_found("Preset not found"))
}

pub async fn delete(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let deleted = state.db.writer.delete_preset(user_id, id).await?;
    Ok(if deleted {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

#[derive(serde::Deserialize)]
pub struct ActivatePresetInput {
    pub preset_id: Option<String>,
}

/// null preset_id = back to aetheria's built-in assembly
pub async fn activate(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<ActivatePresetInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.db.writer.set_active_preset(user_id, input.preset_id).await?;
    Ok(axum::http::StatusCode::OK)
}
