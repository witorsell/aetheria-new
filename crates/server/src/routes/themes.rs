use crate::error::ApiError;
use crate::models::theme::{self, Theme, ThemeExport, ThemeTokens, BUILTIN_DEFAULT_ID, BUILTIN_LIGHT_ID};
use crate::state::AppState;
use axum::extract::{Extension, Multipart, Path, State};
use axum::response::IntoResponse;
use axum::Json;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const MAX_THEME_NAME: usize = 256;
const MAX_CUSTOM_CSS: usize = 100_000;

const AT_IMPORT_WARNING: &str = "The imported theme's custom CSS contained an @import rule, which was removed. @import can be used to load external stylesheets that track or fingerprint you.";
const EXTERNAL_URL_WARNING: &str = "The imported theme's custom CSS referenced an external url(), which was removed. A url() pointing off-site (background-image, @font-face src, cursor, etc.) can be used as a tracking pixel the same way @import can.";

fn at_import_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // case-insensitive (CSS at-keywords are case-insensitive: @IMPORT is
    // just as valid as @import), and the trailing `;` is optional so a
    // payload missing one before EOF still gets matched and removed rather
    // than silently surviving while the caller (wrongly) says it didn't.
    RE.get_or_init(|| Regex::new(r"(?i)@import[^;]*;?").unwrap())
}

/// strips every `@import` at-rule out of `css`, re-scanning its own output
/// until nothing more matches. a single `replace_all` pass can be defeated
/// by a payload crafted so the leftover pieces reform the keyword once the
/// matched substring is cut out (e.g. `@imp@import url(a);ort url(...)`
/// collapses to `@import url(...)` after one pass); looping to a fixed
/// point closes that gap. returns the cleaned css and whether anything was
/// actually removed, so callers never claim a strip that didn't happen.
fn strip_at_import(css: &str) -> (String, bool) {
    let re = at_import_regex();
    let mut current = css.to_string();
    let mut stripped_anything = false;
    loop {
        let next = re.replace_all(&current, "").into_owned();
        if next == current {
            break;
        }
        stripped_anything = true;
        current = next;
    }
    (current.trim().to_string(), stripped_anything)
}

fn external_url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // @import isn't the only way custom CSS can reach off-site: a plain
    // url() in an ordinary property value (background-image, @font-face
    // src, cursor, list-style-image, ...) fetches just as eagerly and is
    // just as usable as a tracking pixel. matches url(...) whose target -
    // after an optional quote and optional http(s): scheme - starts with
    // // (i.e. it's protocol-relative or an absolute http/https URL), so a
    // local/relative path or a self-contained data: URI is left untouched.
    RE.get_or_init(|| Regex::new(r#"(?i)url\(\s*['"]?\s*(?:https?:)?//[^)]*\)"#).unwrap())
}

/// same fixed-point-iteration reasoning as strip_at_import: a single pass
/// can be defeated by a payload whose leftover pieces reform a live url()
/// once the matched substring is cut out.
fn strip_external_urls(css: &str) -> (String, bool) {
    let re = external_url_regex();
    let mut current = css.to_string();
    let mut stripped_anything = false;
    loop {
        let next = re.replace_all(&current, "url()").into_owned();
        if next == current {
            break;
        }
        stripped_anything = true;
        current = next;
    }
    (current.trim().to_string(), stripped_anything)
}

/// validates name/CSS-length limits and strips `@import` and any external
/// `url()` out of `tokens.custom_css` in place. runs uniformly for every
/// path a theme can be created or updated through (create, update, the
/// plain import, the SillyTavern import), so the same untrusted-file threat
/// model gets the same treatment everywhere instead of only where someone
/// remembered to add it. returns a warning if anything was actually stripped.
fn validate(name: &str, tokens: &mut ThemeTokens) -> Result<Option<String>, ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::bad_request("Name cannot be empty"));
    }
    if name.len() > MAX_THEME_NAME {
        return Err(ApiError::bad_request("Name too long (max 256 characters)"));
    }
    if tokens.custom_css.len() > MAX_CUSTOM_CSS {
        return Err(ApiError::bad_request("Custom CSS too long (max 100,000 characters)"));
    }
    let (cleaned, stripped_import) = strip_at_import(&tokens.custom_css);
    let (cleaned, stripped_url) = strip_external_urls(&cleaned);
    tokens.custom_css = cleaned;

    let mut warnings = Vec::new();
    if stripped_import {
        warnings.push(AT_IMPORT_WARNING);
    }
    if stripped_url {
        warnings.push(EXTERNAL_URL_WARNING);
    }
    Ok((!warnings.is_empty()).then(|| warnings.join(" ")))
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
    Json(mut input): Json<CreateThemeInput>,
) -> Result<Json<Theme>, ApiError> {
    if validate(&input.name, &mut input.tokens)?.is_some() {
        tracing::warn!(user_id, "@import and/or external url() stripped from theme custom_css on create");
    }
    let theme = state.db.writer.create_theme(user_id, input.name, input.tokens).await?;
    Ok(Json(theme))
}

pub async fn update(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut input): Json<CreateThemeInput>,
) -> Result<axum::http::StatusCode, ApiError> {
    if validate(&input.name, &mut input.tokens)?.is_some() {
        tracing::warn!(user_id, "@import and/or external url() stripped from theme custom_css on update");
    }
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

async fn read_multipart_file(multipart: &mut Multipart) -> Result<(bytes::Bytes, Option<String>), String> {
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => return Err("No file uploaded".to_string()),
            Err(e) => return Err(format!("Upload failed: {e}")),
        };
        if field.name() == Some("file") {
            let filename = field.file_name().map(|s| s.to_string());
            return field.bytes().await.map(|b| (b, filename)).map_err(|e| format!("Upload failed: {e}"));
        }
    }
}

#[derive(Serialize)]
pub struct ImportResult {
    theme: Theme,
    warning: Option<String>,
}

pub async fn import(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let (bytes, filename) = match read_multipart_file(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    };
    let mut export: ThemeExport = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Invalid theme JSON").into_response(),
    };
    let name = if export.name.trim().is_empty() {
        filename.map(|f| f.trim_end_matches(".json").to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Imported Theme".into())
    } else {
        export.name.clone()
    };
    let warning = match validate(&name, &mut export.tokens) {
        Ok(w) => w,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, format!("{:?}", e)).into_response(),
    };
    match state.db.writer.create_theme(user_id, name, export.tokens).await {
        Ok(theme) => Json(ImportResult { theme, warning }).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to save theme").into_response(),
    }
}

#[derive(Serialize)]
pub struct ImportStResult {
    theme: Theme,
    warning: Option<String>,
}

pub async fn import_st(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let (bytes, filename) = match read_multipart_file(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    };
    let raw: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "Invalid JSON").into_response(),
    };
    let mut tokens = theme::st_to_aetheria(&raw);
    let name = raw.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())
        .or_else(|| filename.map(|f| f.trim_end_matches(".json").to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Imported SillyTavern Theme".into());
    let warning = match validate(&name, &mut tokens) {
        Ok(w) => w,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, format!("{:?}", e)).into_response(),
    };
    match state.db.writer.create_theme(user_id, name, tokens).await {
        Ok(theme) => Json(ImportStResult { theme, warning }).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to save theme").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_uppercase_at_import() {
        let (css, stripped) = strip_at_import("@IMPORT url('https://evil.example/track.css'); .foo { color: red; }");
        assert!(stripped);
        assert!(!css.to_ascii_lowercase().contains("@import"));
        assert!(css.contains(".foo"));
    }

    #[test]
    fn strips_at_import_missing_trailing_semicolon() {
        // no `;` before EOF - the old single-pass, semicolon-required regex
        // left this untouched while still claiming it stripped something.
        let (css, stripped) = strip_at_import(".foo { color: red; } @import url('https://evil.example/track.css')");
        assert!(stripped, "a strip must actually happen, not just be claimed");
        assert!(!css.to_ascii_lowercase().contains("@import"));
    }

    #[test]
    fn strips_reformed_at_import_that_survives_a_single_pass() {
        // splitting the keyword around a chunk that a single replace_all
        // pass would remove: cutting "@import url(a);" out of the middle
        // leaves "@imp" + "ort url(...)" which collapses back into a live
        // @import rule. the fixed point loop must catch the reformed rule.
        let (css, stripped) = strip_at_import("@imp@import url(a);ort url(evil.css); .foo { color: red; }");
        assert!(stripped);
        assert!(!css.to_ascii_lowercase().contains("@import"), "reformed @import survived: {css:?}");
        assert!(css.contains(".foo"));
    }

    #[test]
    fn does_not_claim_a_strip_when_nothing_was_there() {
        let (css, stripped) = strip_at_import(".foo { color: red; }");
        assert!(!stripped);
        assert_eq!(css, ".foo { color: red; }");
    }

    #[test]
    fn validate_strips_and_warns_via_the_shared_path() {
        // this is the same helper create/update/import/import-st all call now,
        // so exercising it here covers the plain (own-format) import endpoint's
        // behavior too - it no longer skips @import handling the way it used to.
        let mut tokens = theme::default_theme_tokens();
        tokens.custom_css = "@import url('https://evil.example/track.css'); .foo { color: red; }".to_string();
        let warning = validate("My Theme", &mut tokens).unwrap();
        assert!(warning.is_some());
        assert!(!tokens.custom_css.to_ascii_lowercase().contains("@import"));
        assert!(tokens.custom_css.contains(".foo"));
    }

    #[test]
    fn validate_is_silent_when_custom_css_is_clean() {
        let mut tokens = theme::default_theme_tokens();
        tokens.custom_css = ".foo { color: red; }".to_string();
        let warning = validate("My Theme", &mut tokens).unwrap();
        assert!(warning.is_none());
        assert_eq!(tokens.custom_css, ".foo { color: red; }");
    }

    #[test]
    fn strips_external_url_in_an_ordinary_property_value() {
        // @import isn't the only door - a plain url() in background-image,
        // @font-face src, cursor, etc. fetches just as eagerly
        let (css, stripped) = strip_external_urls(".foo { background-image: url('https://evil.example/track.png'); }");
        assert!(stripped);
        assert!(!css.contains("evil.example"));
        assert!(css.contains(".foo"));
    }

    #[test]
    fn strips_protocol_relative_external_url() {
        let (css, stripped) = strip_external_urls("@font-face { src: url(//evil.example/font.woff2); }");
        assert!(stripped);
        assert!(!css.contains("evil.example"));
    }

    #[test]
    fn strips_uppercase_external_url() {
        let (css, stripped) = strip_external_urls(".foo { cursor: URL(\"HTTPS://evil.example/x.png\"), pointer; }");
        assert!(stripped);
        assert!(!css.to_ascii_lowercase().contains("evil.example"));
    }

    #[test]
    fn leaves_data_uri_and_relative_urls_alone() {
        let css_in = ".a { background: url(data:image/png;base64,AAAA); } .b { background: url(/local/pic.png); } .c { background: url(pic.png); }";
        let (css, stripped) = strip_external_urls(css_in);
        assert!(!stripped, "data: URIs and local/relative paths aren't a tracking vector and must survive untouched");
        assert_eq!(css, css_in);
    }

    #[test]
    fn does_not_claim_a_url_strip_when_nothing_was_there() {
        let (css, stripped) = strip_external_urls(".foo { color: red; }");
        assert!(!stripped);
        assert_eq!(css, ".foo { color: red; }");
    }

    #[test]
    fn validate_strips_external_url_via_the_shared_path() {
        let mut tokens = theme::default_theme_tokens();
        tokens.custom_css = ".foo { background-image: url('https://evil.example/track.png'); }".to_string();
        let warning = validate("My Theme", &mut tokens).unwrap();
        assert!(warning.is_some());
        assert!(!tokens.custom_css.contains("evil.example"));
    }

    #[test]
    fn validate_combines_warnings_when_both_import_and_url_are_present() {
        let mut tokens = theme::default_theme_tokens();
        tokens.custom_css =
            "@import url('https://evil.example/a.css'); .foo { background: url('https://evil.example/b.png'); }".to_string();
        let warning = validate("My Theme", &mut tokens).unwrap().unwrap();
        assert!(warning.contains("@import"));
        assert!(warning.contains("url()"));
        assert!(!tokens.custom_css.contains("evil.example"));
    }
}
