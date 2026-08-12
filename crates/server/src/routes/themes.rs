use crate::error::ApiError;
use crate::models::theme::{self, Theme, ThemeExport, ThemeTokens, BUILTIN_DEFAULT_ID, BUILTIN_LIGHT_ID};
use crate::state::AppState;
use axum::extract::{Extension, Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

const MAX_THEME_NAME: usize = 256;
const MAX_CUSTOM_CSS: usize = 100_000;

fn validate(name: &str, tokens: &ThemeTokens) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if name.len() > MAX_THEME_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    if tokens.custom_css.len() > MAX_CUSTOM_CSS {
        return Err(ApiError::bad_request("Custom CSS too long (max 100,000 characters)"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ThemeListItem {
    pub id: String,
    pub name: String,
    pub tokens: ThemeTokens,
    pub builtin: bool,
    pub active: bool,
}

pub async fn list(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<ThemeListItem>>, ApiError> {
    let active_id = state.db.writer.get_active_theme_id(user_id).await?;
    let mut items = vec![
        ThemeListItem { id: BUILTIN_DEFAULT_ID.into(), name: "Aetheria".into(), tokens: theme::default_theme_tokens(), builtin: true, active: active_id == BUILTIN_DEFAULT_ID },
        ThemeListItem { id: BUILTIN_LIGHT_ID.into(), name: "Aetheria Light".into(), tokens: theme::light_theme_tokens(), builtin: true, active: active_id == BUILTIN_LIGHT_ID },
    ];
    let custom = theme::list(&state.db.read_pool, user_id).await?;
    items.extend(custom.into_iter().map(|t| {
        let is_active = t.id == active_id;
        ThemeListItem { id: t.id, name: t.name, tokens: t.tokens, builtin: false, active: is_active }
    }));
    Ok(Json(items))
}

pub async fn get_active(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
) -> Result<Json<ThemeTokens>, ApiError> {
    let active_id = state.db.writer.get_active_theme_id(user_id).await?;
    if let Some(builtin) = theme::builtin_by_id(&active_id) {
        return Ok(Json(builtin));
    }
    let resolved = theme::get(&state.db.read_pool, user_id, &active_id)
        .await?
        .map(|t| t.tokens)
        .unwrap_or_else(theme::default_theme_tokens);
    Ok(Json(resolved))
}

pub async fn get_theme(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Theme>, ApiError> {
    theme::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Theme not found"))
}

#[derive(Deserialize)]
pub struct CreateThemeInput {
    pub name: String,
    pub tokens: ThemeTokens,
}

pub async fn create(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<CreateThemeInput>,
) -> Result<Json<Theme>, ApiError> {
    validate(&input.name, &input.tokens)?;
    let theme = state.db.writer.create_theme(user_id, input.name, input.tokens).await?;
    Ok(Json(theme))
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<CreateThemeInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    validate(&input.name, &input.tokens)?;
    let updated = state.db.writer.update_theme(user_id, id, input.name, input.tokens).await?;
    Ok(if updated { axum::http::StatusCode::OK } else { axum::http::StatusCode::NOT_FOUND })
}

pub async fn delete(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let deleted = state.db.writer.delete_theme(user_id, id).await?;
    Ok(if deleted { axum::http::StatusCode::OK } else { axum::http::StatusCode::NOT_FOUND })
}

pub async fn export_theme(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ThemeExport>, ApiError> {
    if let Some(tokens) = theme::builtin_by_id(&id) {
        return Ok(Json(ThemeExport { name: id, tokens }));
    }
    theme::get(&state.db.read_pool, user_id, &id)
        .await?
        .map(|t| Json(t.into()))
        .ok_or_else(|| ApiError::not_found("Theme not found"))
}

#[derive(Deserialize)]
pub struct ActivateThemeInput {
    pub theme_id: String,
}

pub async fn activate(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Json(input): Json<ActivateThemeInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    state.db.writer.set_active_theme(user_id, input.theme_id).await?;
    Ok(axum::http::StatusCode::OK)
}
