use axum::extract::Extension;
use crate::state::AppState;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar};
use serde::Deserialize;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub mod middleware;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

const MAX_LOGIN_ATTEMPTS: u32 = 5;

pub async fn login(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(PrivateCookieJar, StatusCode), StatusCode> {
    tracing::info!("Login attempt for username: {}", req.username);
    let attempt_key = req.username.to_lowercase();
    if state.login_attempts.get(&attempt_key).await.unwrap_or(0) >= MAX_LOGIN_ATTEMPTS {
        tracing::warn!("Login locked out for username: {}", req.username);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let result = try_login(&state, &req).await;
    if result.is_err() {
        let next_count = state.login_attempts.get(&attempt_key).await.unwrap_or(0) + 1;
        state.login_attempts.insert(attempt_key.clone(), next_count).await;
    } else {
        state.login_attempts.invalidate(&attempt_key).await;
    }
    let user = result?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let expires_at = now + (state.session_absolute_max_age.as_millis() as i64);

    state.db.writer.create_session(user.id, session_id.clone(), expires_at)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let max_age_secs = state.session_absolute_max_age.as_secs() as i64;
    let cookie = Cookie::build(("session", session_id))
        .http_only(true)
        .secure(true)
        .path("/")
        .same_site(cookie::SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(max_age_secs))
        .build();

    Ok((jar.add(cookie), StatusCode::OK))
}

async fn try_login(state: &AppState, req: &LoginRequest) -> Result<crate::models::user::User, StatusCode> {
    let user = crate::models::user::find_by_username(&state.db.read_pool, &req.username)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to look up user by username");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            tracing::info!("User not found in DB for username: {}", req.username);
            StatusCode::UNAUTHORIZED
        })?;

    Argon2::default()
        .verify_password(req.password.as_bytes(), &PasswordHash::new(&user.password_hash).map_err(|e| {
            tracing::warn!("Invalid password hash for user {}: {}", req.username, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?)
        .map_err(|e| {
            tracing::error!("Failed to verify password: {}", e);
            StatusCode::UNAUTHORIZED
        })?;
    tracing::info!("Password verified for {}", req.username);

    Ok(user)
}
pub async fn logout(
    Extension(user_id): axum::extract::Extension<i64>,
    State(state): State<AppState>,
    jar: PrivateCookieJar,
) -> (PrivateCookieJar, axum::response::Redirect) {
    tracing::info!("Logout endpoint hit for user {}", user_id);
    if let Some(cookie) = jar.get("session") {
        let _ = state.db.writer.delete_session(user_id, cookie.value().to_string()).await;
    }
    let remove_cookie = Cookie::build(("session", ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(cookie::SameSite::Lax)
        .max_age(cookie::time::Duration::ZERO)
        .build();
    (jar.add(remove_cookie), axum::response::Redirect::to("/"))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct MeResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub persona: Option<String>,
    pub use_persona: bool,
    pub avatar_url: Option<String>,
}

pub async fn me(Extension(user_id): axum::extract::Extension<i64>, State(state): State<AppState>) -> Result<Json<MeResponse>, StatusCode> {
    let user = crate::models::user::find_by_id(&state.db.read_pool, user_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to look up user by id");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(MeResponse {
        username: user.username,
        display_name: user.display_name,
        persona: user.persona,
        use_persona: user.use_persona,
        avatar_url: user.avatar_url,
    }))
}

#[derive(serde::Deserialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
    pub persona: Option<String>,
    pub use_persona: bool,
}

pub async fn update_me(
    Extension(user_id): axum::extract::Extension<i64>,
    State(state): State<AppState>,
    Json(req): Json<UpdateMeRequest>,
) -> Result<Json<MeResponse>, StatusCode> {
    state.db.writer.update_user(user_id, req.display_name, req.persona, req.use_persona)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    me(axum::extract::Extension(user_id), State(state)).await
}



pub fn uploads_dir() -> PathBuf {
    crate::resolve_path("crates/server/uploads")
}

pub async fn upload_my_avatar(
    Extension(user_id): axum::extract::Extension<i64>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let upload_dir = uploads_dir();
    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("failed to create uploads dir: {e}")))?;

    const MAX_AVATAR_SIZE: usize = 20 * 1024 * 1024;
    const ALLOWED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let Some(name) = field.name().map(|s| s.to_string()) else {
            continue;
        };

        let filename = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "avatar".to_string());

        let ext = PathBuf::from(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        if !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
            return Err((StatusCode::BAD_REQUEST, format!("File extension '.{ext}' is not allowed. Allowed: png, jpg, jpeg, gif, webp")));
        }

        let stored_name = format!("avatar_user_{user_id}.{ext}");
        let path = upload_dir.join(&stored_name);

        // chunk-by-chunk size check, not `field.bytes()` which would just
        // buffer the whole upload before we even get to check mAX_AVATAR_SIZE
        let mut data = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("read failed: {e}")))?
        {
            if data.len() + chunk.len() > MAX_AVATAR_SIZE {
                return Err((StatusCode::PAYLOAD_TOO_LARGE, format!("File too large. Maximum size is {} bytes", MAX_AVATAR_SIZE)));
            }
            data.extend_from_slice(&chunk);
        }

        let mut file = fs::File::create(&path)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("create file failed: {e}")))?;
        file.write_all(&data)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}")))?;

        // same cache-busting need as character avatars: the stored filename
        // is stable per user, so without a version query string the URL
        // never changes and a re-upload never gets picked up.
        let version = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let url = format!("/uploads/{stored_name}?v={version}");

        state.db.writer.update_user_avatar(user_id, Some(url.clone()))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        return Ok(Json(serde_json::json!({ "url": url, "field": name })));
    }

    Err((StatusCode::BAD_REQUEST, "no file field found".to_string()))
}

pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut rand::thread_rng());
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

pub async fn register(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Json(req): Json<RegisterRequest>,
) -> Result<(PrivateCookieJar, StatusCode), StatusCode> {
    if !state.registration_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let username = req.username.trim().to_string();
    if username.is_empty() || username.len() > 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.password.len() < 8 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let existing = crate::models::user::find_by_username(&state.db.read_pool, &username).await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to check existing username");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if existing.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let hash = hash_password(&req.password);
    state.db.writer.create_user(username, hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user = crate::models::user::find_by_username(&state.db.read_pool, &req.username.trim())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to find newly created user");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let expires_at = now + (state.session_absolute_max_age.as_millis() as i64);

    state.db.writer.create_session(user.id, session_id.clone(), expires_at)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let max_age_secs = state.session_absolute_max_age.as_secs() as i64;
    let cookie = Cookie::build(("session", session_id))
        .http_only(true)
        .secure(true)
        .path("/")
        .same_site(cookie::SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(max_age_secs))
        .build();

    Ok((jar.add(cookie), StatusCode::OK))
}

pub async fn registration_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "enabled": state.registration_enabled }))
}
