#[tokio::test]
async fn legacy_persona_becomes_default_and_active() {
    std::env::set_var(
        "AETHERIA_SESSION_SECRET",
        "test-secret-at-least-64-bytes-long-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    );
    std::env::set_var("AETHERIA_ENCRYPTION_KEY", "01234567890123456789012345678901"[..32].to_string());

    let db = server::db::connect(":memory:").await;
    server::bootstrap_user(&db, "legacyuser", "test-pass-1234").await;

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("legacyuser")
        .fetch_one(&db.read_pool)
        .await
        .unwrap();

    sqlx::query("UPDATE users SET persona = ?, use_persona = 1 WHERE id = ?")
        .bind("a wandering merchant")
        .bind(user_id)
        .execute(&db.read_pool)
        .await
        .unwrap();

    // re-run just this migration's backfill logic directly, since the
    // migrator already ran once during connect() before the row above existed
    sqlx::query(
        "INSERT INTO personas (id, user_id, name, description, avatar_url, created_at, updated_at)
         SELECT lower(hex(randomblob(16))), id, 'Default', persona, NULL, 0, 0
         FROM users WHERE use_persona = 1 AND persona IS NOT NULL AND trim(persona) != '' AND id = ?",
    )
    .bind(user_id)
    .execute(&db.read_pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE users SET active_persona_id = (SELECT p.id FROM personas p WHERE p.user_id = users.id AND p.name = 'Default') WHERE id = ?",
    )
    .bind(user_id)
    .execute(&db.read_pool)
    .await
    .unwrap();

    let user = server::models::user::find_by_id(&db.read_pool, user_id)
        .await
        .unwrap()
        .expect("user should exist");
    assert!(user.active_persona_id.is_some());

    let persona_desc: String = sqlx::query_scalar("SELECT description FROM personas WHERE id = ?")
        .bind(user.active_persona_id.unwrap())
        .fetch_one(&db.read_pool)
        .await
        .unwrap();
    assert_eq!(persona_desc, "a wandering merchant");
}
