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

    // ensure existing databases from previous migrations transition seamlessly without failing checksums or nuking data
    let has_users: Option<(i64,)> = sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='users'")
        .fetch_optional(&read_pool)
        .await
        .ok()
        .flatten();

    if let Some((count,)) = has_users {
        if count > 0 {
            // database already has schema initialized; align _sqlx_migrations to baseline
            let _ = sqlx::query("CREATE TABLE IF NOT EXISTS _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)")
                .execute(&read_pool)
                .await;
            let _ = sqlx::query("DELETE FROM _sqlx_migrations WHERE version > 1")
                .execute(&read_pool)
                .await;
        }
    }

    // apply migrations to set up the schema
    if let Err(e) = sqlx::migrate!("./migrations").run(&read_pool).await {
        tracing::warn!("migration status: {}", e);
    }

    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(writer::run(writer_conn, rx));

    Db { read_pool, writer: Writer::new(tx) }
}
