use serde::{Deserialize, Serialize};

fn d_color_bg() -> String { "#0a0a0c".into() }
fn d_color_surface() -> String { "transparent".into() }
fn d_color_surface_hover() -> String { "rgba(255, 255, 255, 0.04)".into() }
fn d_color_border() -> String { "rgba(255, 255, 255, 0.08)".into() }
fn d_color_accent() -> String { "#7c5cff".into() }
fn d_color_accent_2() -> String { "#ff9ecf".into() }
fn d_color_accent_hover() -> String { "#9b80ff".into() }
fn d_color_text() -> String { "#ffffff".into() }
fn d_color_text_muted() -> String { "#a09aad".into() }
fn d_color_text_heading() -> String { "#ffffff".into() }
fn d_color_error() -> String { "#f43f5e".into() }
fn d_color_error_bg() -> String { "rgba(244, 63, 94, 0.05)".into() }
fn d_font_heading() -> String { "'Quicksand', system-ui, sans-serif".into() }
fn d_font_body() -> String { "'Inter', system-ui, sans-serif".into() }
fn d_font_scale() -> f64 { 1.0 }
fn d_radius_sm() -> String { "8px".into() }
fn d_radius_md() -> String { "14px".into() }
fn d_radius_lg() -> String { "22px".into() }
fn d_avatar_style() -> String { "rounded".into() }
fn d_blur_strength() -> f64 { 0.0 }
fn d_shadow_strength() -> f64 { 1.0 }
fn d_chat_width() -> f64 { 50.0 }
fn d_chat_display() -> String { "bubble".into() }
fn d_mascot_accent() -> String { "#c084fc".into() }
fn d_true() -> bool { true }

/// the full set of visual knobs a theme controls, applied as CSS custom
/// properties (plus a few body-class toggles) by the frontend's
/// `theme::apply_tokens_to_root`. every field has a `#[serde(default)]` so a
/// partial JSON blob (an older export, a hand-edited file, a SillyTavern
/// theme run through `st_to_aetheria`) still deserializes instead of
/// rejecting the whole import over one missing key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeTokens {
    #[serde(default = "d_color_bg")]
    pub color_bg: String,
    #[serde(default = "d_color_surface")]
    pub color_surface: String,
    #[serde(default = "d_color_surface_hover")]
    pub color_surface_hover: String,
    #[serde(default = "d_color_border")]
    pub color_border: String,
    #[serde(default = "d_color_accent")]
    pub color_accent: String,
    #[serde(default = "d_color_accent_2")]
    pub color_accent_2: String,
    #[serde(default = "d_color_accent_hover")]
    pub color_accent_hover: String,
    #[serde(default = "d_color_text")]
    pub color_text: String,
    #[serde(default = "d_color_text_muted")]
    pub color_text_muted: String,
    #[serde(default = "d_color_text_heading")]
    pub color_text_heading: String,
    #[serde(default = "d_color_error")]
    pub color_error: String,
    #[serde(default = "d_color_error_bg")]
    pub color_error_bg: String,
    #[serde(default = "d_font_heading")]
    pub font_heading: String,
    #[serde(default = "d_font_body")]
    pub font_body: String,
    #[serde(default = "d_font_scale")]
    pub font_scale: f64,
    #[serde(default = "d_radius_sm")]
    pub radius_sm: String,
    #[serde(default = "d_radius_md")]
    pub radius_md: String,
    #[serde(default = "d_radius_lg")]
    pub radius_lg: String,
    /// "circle" | "rounded" | "square"
    #[serde(default = "d_avatar_style")]
    pub avatar_style: String,
    #[serde(default = "d_blur_strength")]
    pub blur_strength: f64,
    #[serde(default = "d_shadow_strength")]
    pub shadow_strength: f64,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default = "d_chat_width")]
    pub chat_width: f64,
    /// "bubble" | "flat"
    #[serde(default = "d_chat_display")]
    pub chat_display: String,
    #[serde(default = "d_true")]
    pub mascot_enabled: bool,
    #[serde(default = "d_mascot_accent")]
    pub mascot_accent: String,
    #[serde(default)]
    pub custom_css: String,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        serde_json::from_str("{}").expect("every field has a serde default")
    }
}

pub const BUILTIN_DEFAULT_ID: &str = "default";
pub const BUILTIN_LIGHT_ID: &str = "light";

pub fn default_theme_tokens() -> ThemeTokens {
    ThemeTokens::default()
}

pub fn light_theme_tokens() -> ThemeTokens {
    ThemeTokens {
        color_bg: "#faf8fc".into(),
        color_surface: "transparent".into(),
        color_surface_hover: "rgba(20, 10, 30, 0.04)".into(),
        color_border: "rgba(20, 10, 30, 0.08)".into(),
        color_accent: "#7c5cff".into(),
        color_accent_2: "#e0409f".into(),
        color_accent_hover: "#6a4de6".into(),
        color_text: "#1a1420".into(),
        color_text_muted: "#6b6376".into(),
        color_text_heading: "#1a1420".into(),
        color_error: "#dc2626".into(),
        color_error_bg: "rgba(220, 38, 38, 0.08)".into(),
        ..ThemeTokens::default()
    }
}

pub fn builtin_by_id(id: &str) -> Option<ThemeTokens> {
    match id {
        BUILTIN_DEFAULT_ID => Some(default_theme_tokens()),
        BUILTIN_LIGHT_ID => Some(light_theme_tokens()),
        _ => None,
    }
}

fn st_avatar_style(n: i64) -> String {
    match n {
        0 => "circle".into(),
        3 => "square".into(),
        _ => "rounded".into(),
    }
}

fn st_chat_display(n: i64) -> String {
    match n {
        0 => "flat".into(),
        _ => "bubble".into(),
    }
}

/// translates a raw SillyTavern theme JSON export onto aetheria's token
/// set. fields aetheria has that ST doesn't (mascot_*, radius_*, the new
/// color_text_heading/color_surface_hover/color_accent_hover/color_error_bg)
/// are left at the default theme's values. `custom_css` is carried across
/// unmodified here. stripping `@import` out of it is the caller's job
/// (`routes::themes::validate`), which runs uniformly for every path a theme
/// can be created or updated through, not just this one.
pub fn st_to_aetheria(raw: &serde_json::Value) -> ThemeTokens {
    let mut tokens = default_theme_tokens();
    let s = |key: &str| raw.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
    let f = |key: &str| raw.get(key).and_then(|v| v.as_f64());
    let i = |key: &str| raw.get(key).and_then(|v| v.as_i64());

    if let Some(v) = s("main_text_color") { tokens.color_text = v; }
    if let Some(v) = s("quote_text_color") { tokens.color_text_muted = v; }
    if let Some(v) = s("blur_tint_color") { tokens.color_surface = v; }
    if let Some(v) = s("border_color") { tokens.color_border = v; }
    if let Some(v) = f("blur_strength") { tokens.blur_strength = v; }
    if let Some(v) = f("shadow_width") { tokens.shadow_strength = v; }
    if let Some(v) = f("font_scale") { tokens.font_scale = v; }
    if let Some(v) = f("chat_width") { tokens.chat_width = v; }
    if let Some(v) = i("avatar_style") { tokens.avatar_style = st_avatar_style(v); }
    if let Some(v) = i("chat_display") { tokens.chat_display = st_chat_display(v); }
    if let Some(v) = s("custom_css") { tokens.custom_css = v; }

    tokens
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ThemeRow {
    id: String,
    user_id: i64,
    name: String,
    token_json: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Theme {
    pub id: String,
    pub user_id: i64,
    pub name: String,
    pub tokens: ThemeTokens,
    pub created_at: i64,
    pub updated_at: i64,
}

/// a theme stripped of everything specific to this install (id, owner,
/// timestamps), the same round-trip shape `PresetExport` uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeExport {
    pub name: String,
    pub tokens: ThemeTokens,
}

impl From<Theme> for ThemeExport {
    fn from(theme: Theme) -> Self {
        ThemeExport { name: theme.name, tokens: theme.tokens }
    }
}

impl From<ThemeRow> for Theme {
    fn from(row: ThemeRow) -> Self {
        Theme {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            tokens: serde_json::from_str(&row.token_json).unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Theme>> {
    sqlx::query_as::<_, ThemeRow>("SELECT * FROM themes WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(Theme::from).collect())
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Theme>> {
    sqlx::query_as::<_, ThemeRow>("SELECT * FROM themes WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(Theme::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tokens_round_trip_through_json() {
        let tokens = default_theme_tokens();
        let json = serde_json::to_string(&tokens).unwrap();
        let back: ThemeTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(back.color_accent, tokens.color_accent);
        assert_eq!(back.font_heading, tokens.font_heading);
    }

    #[test]
    fn partial_json_fills_in_defaults() {
        // simulates an older/foreign export that only sets one field
        let partial = ThemeTokens::default();
        let json = serde_json::to_string(&serde_json::json!({ "color_accent": "#ff0000" })).unwrap();
        let merged: ThemeTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(merged.color_accent, "#ff0000");
        assert_eq!(merged.font_body, partial.font_body);
    }

    #[tokio::test]
    async fn list_and_get_are_scoped_to_user() {
        let db = crate::db::connect(":memory:").await;
        db.writer.create_user("user2".into(), "hash".into()).await.unwrap();

        db.writer
            .create_theme(1, "Mine".into(), default_theme_tokens())
            .await
            .unwrap();
        db.writer
            .create_theme(2, "Someone Else's".into(), default_theme_tokens())
            .await
            .unwrap();

        let mine = list(&db.read_pool, 1).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].name, "Mine");

        let missing = get(&db.read_pool, 2, &mine[0].id).await.unwrap();
        assert!(missing.is_none(), "a theme owned by user 1 must not be readable by user 2");
    }

    #[tokio::test]
    async fn create_update_delete_round_trip() {
        let db = crate::db::connect(":memory:").await;
        db.writer.create_user("user2".into(), "hash".into()).await.unwrap();

        let created = db.writer.create_theme(1, "My Theme".into(), default_theme_tokens()).await.unwrap();
        assert_eq!(created.name, "My Theme");
        assert_eq!(created.tokens.color_accent, default_theme_tokens().color_accent);

        let mut edited_tokens = created.tokens.clone();
        edited_tokens.color_accent = "#00ff00".into();
        let updated = db.writer.update_theme(1, created.id.clone(), "Renamed".into(), edited_tokens).await.unwrap();
        assert!(updated);

        let fetched = get(&db.read_pool, 1, &created.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "Renamed");
        assert_eq!(fetched.tokens.color_accent, "#00ff00");

        let deleted = db.writer.delete_theme(1, created.id.clone()).await.unwrap();
        assert!(deleted);
        assert!(get(&db.read_pool, 1, &created.id).await.unwrap().is_none());

        // deleting/updating someone else's theme is a no-op, not an error
        let other = db.writer.create_theme(2, "Not Yours".into(), default_theme_tokens()).await.unwrap();
        assert!(!db.writer.delete_theme(1, other.id).await.unwrap());
    }

    #[tokio::test]
    async fn set_and_get_active_theme() {
        let db = crate::db::connect(":memory:").await;
        assert_eq!(db.writer.get_active_theme_id(1).await.unwrap(), "default");
        db.writer.set_active_theme(1, "light".into()).await.unwrap();
        assert_eq!(db.writer.get_active_theme_id(1).await.unwrap(), "light");
    }

    #[test]
    fn st_import_maps_known_fields() {
        let raw = serde_json::json!({
            "name": "Cappuccino",
            "blur_strength": 3,
            "main_text_color": "rgba(235,235,235,1)",
            "quote_text_color": "rgba(165,140,115,1)",
            "blur_tint_color": "rgba(34,30,32,0.95)",
            "shadow_color": "rgba(0,0,0,0.3)",
            "shadow_width": 1,
            "border_color": "rgba(80,80,80,0.89)",
            "font_scale": 1,
            "avatar_style": 2,
            "chat_display": 1,
            "chat_width": 50,
            "custom_css": ""
        });
        let tokens = st_to_aetheria(&raw);
        assert_eq!(tokens.color_text, "rgba(235,235,235,1)");
        assert_eq!(tokens.color_border, "rgba(80,80,80,0.89)");
        assert_eq!(tokens.blur_strength, 3.0);
        assert_eq!(tokens.avatar_style, "rounded"); // ST's avatar_style 2
        assert_eq!(tokens.chat_display, "bubble");  // ST's chat_display 1
        // fields aetheria has that ST doesn't fall back to the default theme
        assert_eq!(tokens.mascot_accent, default_theme_tokens().mascot_accent);
        assert_eq!(tokens.color_text_heading, default_theme_tokens().color_text_heading);
    }

    #[test]
    fn st_import_carries_custom_css_through_unstripped() {
        // @import stripping happens once, uniformly, in routes::themes::validate -
        // not here. this just confirms the raw css makes it onto the tokens.
        let raw = serde_json::json!({
            "custom_css": "@import url('https://evil.example/track.css'); .foo { color: red; }"
        });
        let tokens = st_to_aetheria(&raw);
        assert_eq!(tokens.custom_css, "@import url('https://evil.example/track.css'); .foo { color: red; }");
    }
}
