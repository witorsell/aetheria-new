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

    // apply migrations to set up the schema
    sqlx::migrate!("./migrations")
        .run(&read_pool)
        .await
        .expect("migrations should apply");

    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(writer::run(writer_conn, rx));

    Db { read_pool, writer: Writer::new(tx) }
}
