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
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
    // look for an ancestor directory literally named `target` and use its
    // parent as the project root - a fixed "strip 2 levels" only holds for
    // `target/release/server`; a cross-compiled build at
    // `target/<triple>/release/server` (or any other profile/target nesting
    // cargo introduces) sits one level deeper and would silently resolve to
    // the wrong directory instead of failing loudly
    let root = exe
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "target"))
        .and_then(|target_dir| target_dir.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            // no `target` ancestor - expected for a standalone deployment
            // where the binary was copied out of the cargo tree, but if
            // that's not what's happening this is exactly the "silently
            // wrong directory" case AETHERIA_ROOT exists to avoid
            tracing::warn!(
                exe = %exe.display(),
                "no `target` directory found above the running binary; falling back to its own directory as the project root - set AETHERIA_ROOT explicitly if this is wrong",
            );
            exe.parent().unwrap_or_else(|| std::path::Path::new(".")).to_path_buf()
        });
    root.join(relative)
}

pub async fn bootstrap_user(db: &db::Db, username: &str, password: &str) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db.read_pool)
        .await
        .expect("failed to count users");

    // migration 0011_multi_user.sql seeds a placeholder id=1/'admin'/empty-hash
    // row on every fresh database (it exists to satisfy the user_id foreign
    // keys added later in that same migration for pre-existing single-user
    // installs being upgraded). on a genuinely new install that placeholder is
    // the only row, and it isn't a real bootstrapped account, so it must not
    // block first-run bootstrap the way a real user would.
    let only_unclaimed_placeholder = count == 1
        && sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM users WHERE id = 1 AND username = 'admin' AND password_hash = ''",
        )
        .fetch_one(&db.read_pool)
        .await
        .expect("failed to check for the migration placeholder user")
        == 1;

    if count > 0 && !only_unclaimed_placeholder {
        tracing::info!("users already exist, skipping bootstrap");
        return;
    }
    let hash = auth::hash_password(password);
    db.writer
        .upsert_user(username.to_string(), hash)
        .await
        .expect("bootstrapping the initial user should not fail");
    // init settings for the new user
    db.writer.touch_settings().await.ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bootstrap_claims_the_migration_placeholder_on_a_fresh_database() {
        let db = db::connect(":memory:").await;

        // migration 0011 already seeded id=1/'admin'/'' at this point; a naive
        // "any users exist" check would wrongly treat that as already-bootstrapped
        let placeholder_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE id = 1 AND username = 'admin' AND password_hash = ''",
        )
        .fetch_one(&db.read_pool)
        .await
        .unwrap();
        assert_eq!(placeholder_count, 1, "test assumes the migration placeholder exists before bootstrap runs");

        bootstrap_user(&db, "testuser", "test-pass-1234").await;

        let renamed = models::user::find_by_username(&db.read_pool, "testuser").await.unwrap();
        assert!(renamed.is_some(), "bootstrap should have claimed the placeholder row as 'testuser'");

        let stale_admin = models::user::find_by_username(&db.read_pool, "admin").await.unwrap();
        assert!(stale_admin.is_none(), "the placeholder 'admin' row should have been renamed, not left behind alongside the real user");

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db.read_pool).await.unwrap();
        assert_eq!(total, 1, "claiming the placeholder must not leave two rows behind");
    }

    #[tokio::test]
    async fn bootstrap_does_not_overwrite_a_real_existing_user() {
        let db = db::connect(":memory:").await;
        bootstrap_user(&db, "testuser", "test-pass-1234").await;

        // a second bootstrap call (e.g. env vars changed on restart) must not
        // touch the now-real account
        bootstrap_user(&db, "someone-else", "different-password").await;

        let bootstrapped = models::user::find_by_username(&db.read_pool, "testuser").await.unwrap();
        assert!(bootstrapped.is_some(), "the real user from the first bootstrap must survive a second bootstrap call");
        let someone_else = models::user::find_by_username(&db.read_pool, "someone-else").await.unwrap();
        assert!(someone_else.is_none(), "a second bootstrap call must not create or rename into a new account");
    }
}
