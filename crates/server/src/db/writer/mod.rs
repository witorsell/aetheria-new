use futures_util::FutureExt;
use sqlx::{Connection, SqliteConnection};
use std::panic::AssertUnwindSafe;
use tokio::sync::{mpsc, oneshot};

type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub struct WriteCommand(Box<dyn for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, ()> + Send>);

pub async fn run(mut conn: SqliteConnection, mut rx: mpsc::Receiver<WriteCommand>) {
    while let Some(WriteCommand(job)) = rx.recv().await {
        let fut = AssertUnwindSafe(job(&mut conn));
        let pinned = AssertUnwindSafe(std::pin::pin!(fut));
        if futures_util::FutureExt::catch_unwind(pinned).await.is_err() {
            tracing::error!("writer job panicked, aborting process to prevent database corruption");
            std::process::exit(1);
        }
    }
}

fn chrono_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct Writer {
    tx: mpsc::Sender<WriteCommand>,
}

impl Writer {
    pub fn new(tx: mpsc::Sender<WriteCommand>) -> Self {
        Self { tx }
    }

    async fn dispatch<T, F>(&self, f: F) -> sqlx::Result<T>
    where
        T: Send + 'static,
        F: for<'c> FnOnce(&'c mut SqliteConnection) -> BoxFuture<'c, sqlx::Result<T>> + Send + 'static,
    {
        let (respond_to, rx) = oneshot::channel();
        let job = WriteCommand(Box::new(move |conn| {
            Box::pin(async move {
                let result = f(conn).await;
                let _ = respond_to.send(result);
            })
        }));
        if self.tx.send(job).await.is_err() {
            tracing::error!("writer task has stopped, write dropped");
            return Err(sqlx::Error::PoolClosed);
        }
        match rx.await {
            Ok(result) => result,
            Err(_) => {
                tracing::error!("writer job panicked or response dropped");
                Err(sqlx::Error::PoolClosed)
            }
        }
    }
}

mod account;
mod characters;
mod chats;
mod groups;
mod lorebooks;
mod memory;
mod messages;
mod presets;
mod regex_scripts;
mod sessions;
mod settings;
mod themes;
mod users;
