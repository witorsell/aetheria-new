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
