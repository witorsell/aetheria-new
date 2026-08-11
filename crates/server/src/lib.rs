pub mod auth;
pub mod crypto;
pub mod db;
pub mod error;
pub mod embedding;
pub mod group_activation;
#[cfg(feature = "local-embeddings")]
pub mod local_embedding;
pub mod memory;
pub mod models;
pub mod provider;
pub mod reasoning;
pub mod routes;
pub mod state;
pub mod tokenizer;
pub mod vector_memory;

/// resolve a path relative to the project root. checks env var first
/// (for standalone deployments where the binary is copied elsewhere),
/// then falls back to walking up from the executable location.
pub fn resolve_path(relative: &str) -> std::path::PathBuf {
    // env override: AETHERIA_ROOT=/opt/aetheria
    if let Ok(root) = std::env::var("AETHERIA_ROOT") {
        return std::path::PathBuf::from(root).join(relative);
    }
    // walk up from current_exe to find project root
    let mut p = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    if p.ends_with("release") || p.ends_with("debug") {
        if let Some(grandparent) = p.parent().and_then(|parent| parent.parent()) {
            p = grandparent.to_path_buf();
        }
    }
    p.join(relative)
}

pub async fn bootstrap_user(db: &db::Db, username: &str, password: &str) {
    let hash = auth::hash_password(password);
    db.writer
        .upsert_user(username.to_string(), hash)
        .await
        .expect("bootstrapping the initial user should not fail");
}
