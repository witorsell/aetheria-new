use crate::error::ApiError;
use crate::models::regex_script::RegexScriptInput;
use crate::state::AppState;
use axum::extract::{Extension, Multipart, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::Value;

const MAX_SCRIPT_NAME: usize = 256;
const MAX_REGEX_PATTERN: usize = 10_000;
const MAX_REPLACE_STRING: usize = 10_000;
const MAX_TRIM_STRINGS: usize = 10_000;
const MAX_PLACEMENT: usize = 100;

fn validate_regex_script_input(input: &RegexScriptInput) -> Result<(), ApiError> {
    if input.script_name.trim().is_empty() {
        return Err(ApiError::bad_request("script_name cannot be empty"));
    }
    if input.script_name.len() > MAX_SCRIPT_NAME {
        return Err(ApiError::bad_request("script_name too long (max 256 characters)"));
    }
    if input.find_regex.len() > MAX_REGEX_PATTERN {
        return Err(ApiError::bad_request("find_regex too long (max 10,000 characters)"));
    }
    if input.replace_string.len() > MAX_REPLACE_STRING {
        return Err(ApiError::bad_request("replace_string too long (max 10,000 characters)"));
    }
    if input.trim_strings.len() > MAX_TRIM_STRINGS {
        return Err(ApiError::bad_request("Too many trim_strings (max 10,000)"));
    }
    for s in &input.trim_strings {
        if s.len() > 256 {
            return Err(ApiError::bad_request("trim_strings item too long (max 256 characters)"));
        }
    }
    if input.placement.len() > MAX_PLACEMENT {
        return Err(ApiError::bad_request("Too many placement values (max 100)"));
    }
    if let Some(min_depth) = input.min_depth {
        if min_depth < 0 {
            return Err(ApiError::bad_request("min_depth cannot be negative"));
        }
    }
    if let Some(max_depth) = input.max_depth {
        if max_depth < 0 {
            return Err(ApiError::bad_request("max_depth cannot be negative"));
        }
    }
    if let (Some(min_depth), Some(max_depth)) = (input.min_depth, input.max_depth) {
        if min_depth > max_depth {
            return Err(ApiError::bad_request("min_depth cannot be greater than max_depth"));
        }
    }
    Ok(())
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::regex_script::RegexScript>>, ApiError> {
    Ok(Json(
        crate::models::regex_script::list(&state.db.read_pool, user_id).await?,
    ))
}

/// accepts either a single script object or a JSON array of them: exported
/// one at a time from SillyTavern's UI, but a pack could bundle several.
pub async fn import(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST, format!("Upload failed: {e}")).into_response(),
        };
        if field.name() == Some("file") {
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

    let inputs: Vec<RegexScriptInput> = if raw.is_array() {
        match serde_json::from_value(raw) {
            Ok(v) => v,
            Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Could not parse the regex scripts").into_response(),
        }
    } else {
        match serde_json::from_value::<RegexScriptInput>(raw) {
            Ok(v) => vec![v],
            Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Could not parse the regex script").into_response(),
        }
    };

    let mut created = Vec::new();
    for input in inputs {
        if let Err(e) = validate_regex_script_input(&input) {
            return (axum::http::StatusCode::BAD_REQUEST, format!("Invalid regex script: {:?}", e)).into_response();
        }
        if let Ok(script) = state.db.writer.create_regex_script(user_id, input).await {
            created.push(script);
        }
    }

    Json(created).into_response()
}

/// all of this user's regex scripts, in the exact shape `import` accepts
/// back (a JSON array), so exporting and re-importing round-trips them.
pub async fn export_all(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::regex_script::RegexScriptInput>>, ApiError> {
    let scripts = crate::models::regex_script::list(&state.db.read_pool, user_id)
        .await?;
    Ok(Json(scripts.into_iter().map(Into::into).collect()))
}

pub async fn delete(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let deleted = state.db.writer.delete_regex_script(user_id, id).await?;
    Ok(if deleted {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}

#[derive(serde::Deserialize)]
pub struct SetDisabledInput {
    disabled: bool,
}

pub async fn set_disabled(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<SetDisabledInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    let updated = state.db.writer.set_regex_script_disabled(user_id, id, input.disabled).await?;
    Ok(if updated {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::NOT_FOUND
    })
}
