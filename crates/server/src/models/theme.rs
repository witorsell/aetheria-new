use serde::{Deserialize, Serialize};

fn d_color_bg() -> String { "#0a0a0c".into() }
fn d_color_surface() -> String { "transparent".into() }
fn d_color_border() -> String { "rgba(255, 255, 255, 0.08)".into() }
fn d_color_accent() -> String { "#7c5cff".into() }
fn d_color_accent_2() -> String { "#ff9ecf".into() }
fn d_color_text() -> String { "#ffffff".into() }
fn d_color_text_muted() -> String { "#a09aad".into() }
fn d_color_error() -> String { "#f43f5e".into() }
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
    #[serde(default = "d_color_border")]
    pub color_border: String,
    #[serde(default = "d_color_accent")]
    pub color_accent: String,
    #[serde(default = "d_color_accent_2")]
    pub color_accent_2: String,
    #[serde(default = "d_color_text")]
    pub color_text: String,
    #[serde(default = "d_color_text_muted")]
    pub color_text_muted: String,
    #[serde(default = "d_color_error")]
    pub color_error: String,
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
        color_border: "rgba(20, 10, 30, 0.08)".into(),
        color_accent: "#7c5cff".into(),
        color_accent_2: "#e0409f".into(),
        color_text: "#1a1420".into(),
        color_text_muted: "#6b6376".into(),
        color_error: "#dc2626".into(),
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
}
