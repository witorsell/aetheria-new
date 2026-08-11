pub mod writer;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Connection, SqliteConnection, SqlitePool};
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;
use writer::Writer;

pub struct Db {
    pub read_pool: SqlitePool,
    pub writer: Writer,
}

pub async fn connect(path: &str) -> Db {
    // map memory paths to unique shared cache uris to fix test isolation
    let owned_path;
    let path = if path == ":memory:" {
        owned_path = format!("file:{}?mode=memory&cache=shared", uuid::Uuid::new_v4());
        owned_path.as_str()
    } else {
        path
    };

    let options = SqliteConnectOptions::from_str(path)
        .expect("valid sqlite connect string")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    let read_pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.clone())
        .await
        .expect("read pool should connect");

    let writer_conn: SqliteConnection = SqliteConnection::connect_with(&options)
        .await
        .expect("writer connection should open");

    // get the checksum sqlx expects for our baseline migration from the embedded migrator
    let migrator = sqlx::migrate!("./migrations");
    let base_checksum: Vec<u8> = migrator
        .migrations
        .first()
        .map(|m| m.checksum.to_vec())
        .unwrap_or_default();

    // check if _sqlx_migrations already has the correct baseline
    let has_baseline: Option<(i64,)> = sqlx::query_as(
        "SELECT count(*) FROM _sqlx_migrations WHERE version = 1 AND checksum = ?",
    )
    .bind(base_checksum.as_slice())
    .fetch_optional(&read_pool)
    .await
    .ok()
    .flatten();

    if has_baseline.map_or(true, |(c,)| c == 0) {
        // stale migration records or table doesn't exist yet;
        // if schema is already present (old db), just fix the tracking table
        let has_schema: Option<(i64,)> = sqlx::query_as(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='users'",
        )
        .fetch_optional(&read_pool)
        .await
        .ok()
        .flatten();

        if has_schema.map_or(false, |(c,)| c > 0) {
            tracing::info!(
                "existing database detected, reconciling _sqlx_migrations to squashed baseline"
            );
            let _ = sqlx::query("DELETE FROM _sqlx_migrations")
                .execute(&read_pool)
                .await;
            let _ = sqlx::query(
                "INSERT INTO _sqlx_migrations \
                 (version, description, installed_on, checksum, execution_time) \
                 VALUES (1, 'init', CURRENT_TIMESTAMP, ?, 0)",
            )
            .bind(base_checksum.as_slice())
            .execute(&read_pool)
            .await;
        } else {
            // no schema yet, run migrations to create it
            migrator
                .run(&read_pool)
                .await
                .expect("migrations should apply");
        }
    }

    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(writer::run(writer_conn, rx));

    Db { read_pool, writer: Writer::new(tx) }
}
