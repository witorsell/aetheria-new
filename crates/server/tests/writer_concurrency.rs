#[tokio::test]
async fn concurrent_writes_all_succeed() {
    let db = server::db::connect(":memory:?cache=shared").await;

    let mut handles = Vec::new();
    for _ in 0..50 {
        let writer = db.writer.clone();
        handles.push(tokio::spawn(async move { writer.touch_settings().await }));
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "write should not fail under concurrency: {result:?}");
    }
}

// memory paths should map to shared cache so pools share state
#[tokio::test]
async fn test_shared_memory_path() {
    let db = server::db::connect(":memory:").await;

    db.writer.touch_settings().await.expect("write should succeed");

    let updated_at: i64 = sqlx::query_scalar("SELECT updated_at FROM settings WHERE id = 1")
        .fetch_one(&db.read_pool)
        .await
        .expect("read pool should see the write");

    assert!(updated_at > 0, "read pool should observe the writer's update, got {updated_at}");
}

// each concurrently-created user must get a settings row pointing at their
// own id, not whichever user happened to be newest by the time the second
// of two separate dispatches for one create_user call finally ran
#[tokio::test]
async fn concurrent_create_user_calls_each_get_their_own_settings_row() {
    let db = server::db::connect(":memory:").await;

    // a fresh db already has migration 0011's placeholder admin/settings
    // row seeded, so count deltas rather than assuming a clean baseline
    let initial_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db.read_pool).await.unwrap();
    let initial_settings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings").fetch_one(&db.read_pool).await.unwrap();

    let mut handles = Vec::new();
    for i in 0..20 {
        let writer = db.writer.clone();
        handles.push(tokio::spawn(async move {
            writer.create_user(format!("user{i}"), "hash".to_string()).await
        }));
    }
    for handle in handles {
        handle.await.unwrap().expect("create_user should not fail under concurrency");
    }

    let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&db.read_pool)
        .await
        .unwrap();
    assert_eq!(user_count - initial_users, 20, "all 20 users should have been created");

    let settings_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
        .fetch_one(&db.read_pool)
        .await
        .unwrap();
    assert_eq!(settings_count - initial_settings, 20, "every user must get exactly one settings row, not fewer (dropped) or more (duplicated)");

    let orphaned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM settings s WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.id = s.user_id)",
    )
    .fetch_one(&db.read_pool)
    .await
    .unwrap();
    assert_eq!(orphaned, 0, "every settings row must point at a real user, not a stale MAX(id) snapshot from a racing insert");
}

// connections should get isolated memory dbs to avoid cross-test contamination
#[tokio::test]
async fn test_isolated_memory_connections() {
    let db_a = server::db::connect(":memory:").await;
    let db_b = server::db::connect(":memory:").await;

    db_a.writer.touch_settings().await.expect("write to db_a should succeed");

    let updated_at_a: i64 = sqlx::query_scalar("SELECT updated_at FROM settings WHERE id = 1")
        .fetch_one(&db_a.read_pool)
        .await
        .expect("db_a's own read pool should see its own write");
    assert!(updated_at_a > 0, "db_a should observe its own write, got {updated_at_a}");

    let updated_at_b: i64 = sqlx::query_scalar("SELECT updated_at FROM settings WHERE id = 1")
        .fetch_one(&db_b.read_pool)
        .await
        .expect("db_b should still have its own freshly migrated settings row");
    assert_eq!(
        updated_at_b, 0,
        "db_b should NOT see db_a's write, they must be isolated databases, got {updated_at_b}"
    );
}
