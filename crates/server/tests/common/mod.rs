use tower::ServiceExt;

/// login's handler requires ConnectInfo<SocketAddr> for per-IP rate
/// limiting, which normally only gets populated by
/// into_make_service_with_connect_info() at the real hyper connection level -
/// a bare Router::oneshot() call never provides it. layering a fixed
/// Extension(ConnectInfo(..)) onto the test router satisfies the extractor
/// the same way a real connection would, without every test needing to know
/// or care about it.
pub fn with_fake_connect_info(app: axum::Router) -> axum::Router {
    let fake_addr: std::net::SocketAddr = ([127, 0, 0, 1], 0).into();
    app.layer(axum::extract::Extension(axum::extract::ConnectInfo(fake_addr)))
}

/// logs into `app` as `username`/`password` and returns the session cookie
/// from the response. shared by `authed_app` and `authed_app_with_second_user`
/// so both log in the same way.
async fn login(app: &axum::Router, username: &str, password: &str) -> String {
    let login_body = serde_json::json!({"username": username, "password": password}).to_string();
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/login")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(login_body))
                .unwrap(),
        )
        .await
        .unwrap();
    response
        .headers()
        .get(axum::http::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string()
}

pub async fn authed_app() -> (axum::Router, String) {
    std::env::set_var(
        "AETHERIA_SESSION_SECRET",
        "test-secret-at-least-64-bytes-long-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    );
    std::env::set_var("AETHERIA_ENCRYPTION_KEY", "01234567890123456789012345678901");

    let db = server::db::connect(":memory:").await;
    server::bootstrap_user(&db, "testuser", "test-pass-1234").await;
    // insert a settings row for the bootstrapped user (tests run on fresh DBs)
    sqlx::query("INSERT OR IGNORE INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, updated_at) VALUES (1, '', '', '', '', 8192, '', 0, 0)")
        .execute(&db.read_pool)
        .await
        .ok();
    let app = with_fake_connect_info(server::routes::build_router(server::state::AppState::new(db, true)));

    let cookie = login(&app, "testuser", "test-pass-1234").await;

    (app, cookie)
}

/// like `authed_app`, but also bootstraps a second real user sharing the
/// SAME database and router, and logs them both in. `bootstrap_user` (via
/// `upsert_user`) always writes to user id 1, so the second user is inserted
/// directly here with id 2. use this whenever a test needs to exercise a
/// genuine cross-tenant request (e.g. user b trying to reach user a's data)
/// rather than just a nonexistent-ID 404.
pub async fn authed_app_with_second_user() -> (axum::Router, String, String) {
    std::env::set_var(
        "AETHERIA_SESSION_SECRET",
        "test-secret-at-least-64-bytes-long-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    );
    std::env::set_var("AETHERIA_ENCRYPTION_KEY", "01234567890123456789012345678901");

    let db = server::db::connect(":memory:").await;
    server::bootstrap_user(&db, "testuser", "test-pass-1234").await;
    sqlx::query("INSERT OR IGNORE INTO settings (user_id, api_base_url, api_key, model_name, system_prompt, context_limit, post_history_instructions, forbid_external_media, updated_at) VALUES (1, '', '', '', '', 8192, '', 0, 0)")
        .execute(&db.read_pool)
        .await
        .ok();

    let second_password_hash = server::auth::hash_password("also-test-pass-1234");
    sqlx::query("INSERT INTO users (id, username, password_hash, session_secret) VALUES (2, ?, ?, '')")
        .bind("dana")
        .bind(second_password_hash)
        .execute(&db.read_pool)
        .await
        .expect("inserting the second user should not fail");

    let app = with_fake_connect_info(server::routes::build_router(server::state::AppState::new(db, true)));

    let first_cookie = login(&app, "testuser", "test-pass-1234").await;
    let second_cookie = login(&app, "dana", "also-test-pass-1234").await;

    (app, first_cookie, second_cookie)
}

/// hand-builds a `multipart/form-data` body carrying a single file field,
/// since there's no multipart-building crate in the test dependencies.
pub fn multipart_body(field_name: &str, filename: &str, content_type: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = "aetheria-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{filename}\"\r\n").as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}
