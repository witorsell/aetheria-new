use axum::extract::{Extension, Path, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::ServeFile;

const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

pub fn character_filename<'a>(character_id: &str, url: &'a str) -> Option<&'a str> {
    let filename = url.strip_prefix("/uploads/")?.split('?').next()?;
    let (stem, ext) = filename.rsplit_once('.')?;
    (stem == format!("avatar_{character_id}") && EXTENSIONS.contains(&ext))
        .then_some(filename)
}

pub async fn is_owned(pool: &sqlx::SqlitePool, user_id: i64, filename: &str) -> sqlx::Result<bool> {
    let Some((stem, ext)) = filename.rsplit_once('.') else { return Ok(false) };
    if filename.contains('/') || filename.contains('\\') || !EXTENSIONS.contains(&ext) {
        return Ok(false);
    }
    if stem == format!("avatar_user_{user_id}") {
        return Ok(true);
    }
    let (query, id) = if let Some(id) = stem.strip_prefix("avatar_") {
        ("SELECT EXISTS(SELECT 1 FROM characters WHERE id = ? AND user_id = ?)", id)
    } else if let Some(id) = stem.strip_prefix("persona_") {
        ("SELECT EXISTS(SELECT 1 FROM personas WHERE id = ? AND user_id = ?)", id)
    } else {
        return Ok(false);
    };
    if uuid::Uuid::parse_str(id).is_err() {
        return Ok(false);
    }
    sqlx::query_scalar(sqlx::AssertSqlSafe(query)).bind(id).bind(user_id).fetch_one(pool).await
}

pub async fn serve(
    Extension(user_id): Extension<i64>,
    State(state): State<crate::state::AppState>,
    Path(filename): Path<String>,
    request: Request,
) -> Result<Response, StatusCode> {
    if !is_owned(&state.db.read_pool, user_id, &filename).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(StatusCode::NOT_FOUND);
    }
    let response = ServeFile::new(crate::auth::uploads_dir().join(filename))
        .oneshot(request).await;
    let mut response = match response {
        Ok(response) => response.into_response(),
        Err(error) => match error {},
    };
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    Ok(response)
}
