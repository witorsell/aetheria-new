use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ThemeTokens {
    pub color_bg: String,
    pub color_surface: String,
    pub color_border: String,
    pub color_accent: String,
    pub color_accent_2: String,
    pub color_text: String,
    pub color_text_muted: String,
    pub color_error: String,
    pub font_heading: String,
    pub font_body: String,
    pub font_scale: f64,
    pub radius_sm: String,
    pub radius_md: String,
    pub radius_lg: String,
    pub avatar_style: String,
    pub blur_strength: f64,
    pub shadow_strength: f64,
    pub reduced_motion: bool,
    pub chat_width: f64,
    pub chat_display: String,
    pub mascot_enabled: bool,
    pub mascot_accent: String,
    pub custom_css: String,
}

// mirrors crates/server/src/models/theme.rs's default_theme_tokens(); the frontend
// can't share Rust types with the backend, so these two lists of literals have to be
// kept in sync by hand (same situation api/presets.rs already lives with)
impl Default for ThemeTokens {
    fn default() -> Self {
        ThemeTokens {
            color_bg: "#0a0a0c".into(),
            color_surface: "transparent".into(),
            color_border: "rgba(255, 255, 255, 0.08)".into(),
            color_accent: "#7c5cff".into(),
            color_accent_2: "#ff9ecf".into(),
            color_text: "#ffffff".into(),
            color_text_muted: "#a09aad".into(),
            color_error: "#f43f5e".into(),
            font_heading: "'Quicksand', system-ui, sans-serif".into(),
            font_body: "'Inter', system-ui, sans-serif".into(),
            font_scale: 1.0,
            radius_sm: "8px".into(),
            radius_md: "14px".into(),
            radius_lg: "22px".into(),
            avatar_style: "rounded".into(),
            blur_strength: 0.0,
            shadow_strength: 1.0,
            reduced_motion: false,
            chat_width: 50.0,
            chat_display: "bubble".into(),
            mascot_enabled: true,
            mascot_accent: "#c084fc".into(),
            custom_css: String::new(),
        }
    }
}

#[derive(Clone, Deserialize)]
pub struct ThemeListItem {
    pub id: String,
    pub name: String,
    pub tokens: ThemeTokens,
    pub builtin: bool,
    pub active: bool,
}

#[derive(Clone, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub tokens: ThemeTokens,
}

#[derive(Serialize)]
struct CreateThemeInput<'a> {
    name: &'a str,
    tokens: &'a ThemeTokens,
}

#[derive(Serialize)]
struct ActivateThemeInput<'a> {
    theme_id: &'a str,
}

#[derive(Deserialize)]
struct ImportStResult {
    theme: Theme,
    warning: Option<String>,
}

pub async fn list_themes() -> Result<Vec<ThemeListItem>, String> {
    Request::get("/api/themes")
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn get_active_theme() -> Result<ThemeTokens, String> {
    Request::get("/api/themes/active")
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn get_theme(id: &str) -> Result<Theme, String> {
    Request::get(&format!("/api/themes/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn create_theme(name: &str, tokens: &ThemeTokens) -> Result<Theme, String> {
    Request::post("/api/themes")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&CreateThemeInput { name, tokens }).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn update_theme(id: &str, name: &str, tokens: &ThemeTokens) -> Result<(), String> {
    Request::put(&format!("/api/themes/{}", id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&CreateThemeInput { name, tokens }).map_err(|e| e.to_string())?
        .send().await.map(|_| ()).map_err(|e| e.to_string())
}

pub async fn delete_theme(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/themes/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map(|_| ()).map_err(|e| e.to_string())
}

pub async fn export_theme(id: &str) -> Result<String, String> {
    Request::get(&format!("/api/themes/{}/export", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map_err(|e| e.to_string())?
        .text().await.map_err(|e| e.to_string())
}

pub async fn import_theme(file: web_sys::File) -> Result<Theme, String> {
    let form = web_sys::FormData::new().map_err(|_| "Failed to create FormData")?;
    form.append_with_blob("file", &file).map_err(|_| "Failed to append file")?;
    let resp = Request::post("/api/themes/import")
        .credentials(web_sys::RequestCredentials::Include)
        .body(form).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn import_st_theme(file: web_sys::File) -> Result<(Theme, Option<String>), String> {
    let form = web_sys::FormData::new().map_err(|_| "Failed to create FormData")?;
    form.append_with_blob("file", &file).map_err(|_| "Failed to append file")?;
    let resp = Request::post("/api/themes/import-st")
        .credentials(web_sys::RequestCredentials::Include)
        .body(form).map_err(|e| e.to_string())?
        .send().await.map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    let result: ImportStResult = resp.json().await.map_err(|e| e.to_string())?;
    Ok((result.theme, result.warning))
}

pub async fn activate_theme(id: &str) -> Result<(), String> {
    Request::post("/api/themes/activate")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&ActivateThemeInput { theme_id: id }).map_err(|e| e.to_string())?
        .send().await.map(|_| ()).map_err(|e| e.to_string())
}
