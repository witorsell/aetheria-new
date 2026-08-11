#[tokio::main]
async fn main() {
    dotenvy::dotenv_override().ok();
    tracing_subscriber::fmt::init();



    let db = server::db::connect(
        server::resolve_path("crates/server/aetheria.sqlite3")
            .to_str()
            .unwrap(),
    )
    .await;

    let username = std::env::var("INITIAL_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = std::env::var("INITIAL_PASSWORD").unwrap_or_else(|_| "password".to_string());
    if !username.trim().is_empty() && !password.trim().is_empty() {
        server::bootstrap_user(&db, &username, &password).await;
    }

    let registration_enabled = std::env::var("ENABLE_REGISTRATION")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    tracing::info!("registration enabled: {registration_enabled}");

    let state = server::state::AppState::new(db, registration_enabled);

    // periodic purge of expired and abandoned sessions every hour
    {
        let writer = state.db.writer.clone();
        let idle_timeout = state.session_idle_timeout.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
            loop {
                interval.tick().await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let idle_cutoff = now - (idle_timeout.as_millis() as i64);
                match writer.purge_expired_sessions(now, idle_cutoff).await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            tracing::info!(deleted, "purged expired sessions");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "failed to purge expired sessions"),
                }
            }
        });
    }

    let app = server::routes::build_router(state);

    let bind_addr = std::env::var("AETHERIA_BIND").unwrap_or_else(|_| "127.0.0.1:4310".to_string());
    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("failed to bind to {bind_addr}: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!("listening on {}", listener.local_addr().map(|a| a.to_string()).unwrap_or(bind_addr));
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("server error: {e}");
        std::process::exit(1);
    }
}
