//! a numbered log of everything a reply streamed, so a client whose live
//! connection went quiet can pick up from the last event it saw.
//!
//! the connection from this box to Cloudflare freezes once a response gets
//! past about 16KB or a few dozen packets, and a reply stream is one long
//! response. the log lives next to the live stream: every event is written
//! here first, then offered to the stream. a client that stops hearing from
//! the stream asks `GET .../generation/{id}/events?from=N` instead, which is
//! a small answer on a fresh connection.

use axum::response::sse::Event;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, Notify};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Data,
    Member,
    Error,
    Done,
}

impl Kind {
    fn tag(self) -> &'static str {
        match self {
            Kind::Data => "data",
            Kind::Member => "member",
            Kind::Error => "error",
            Kind::Done => "done",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LoggedEvent {
    pub k: &'static str,
    pub d: String,
}

#[derive(Default)]
struct Inner {
    events: Vec<LoggedEvent>,
    finished: bool,
}

pub struct LiveGeneration {
    pub user_id: i64,
    pub chat_id: String,
    inner: Mutex<Inner>,
    wake: Notify,
}

/// what one poll gets back
#[derive(Debug, PartialEq)]
pub struct Slice {
    pub events: Vec<LoggedEvent>,
    /// the index to ask for next
    pub next: usize,
    /// nothing more will ever be added after `next`
    pub done: bool,
}

/// a poll answer stays small enough to be one quick response on its own
/// connection, well under the point where the path freezes
const MAX_SLICE_BYTES: usize = 6 * 1024;

impl LiveGeneration {
    pub fn new(user_id: i64, chat_id: String) -> Arc<Self> {
        Arc::new(Self { user_id, chat_id, inner: Mutex::new(Inner::default()), wake: Notify::new() })
    }

    /// adds an event and returns its position in the log, or None when the
    /// generation is already over
    fn push(&self, kind: Kind, data: String) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();
        if inner.finished {
            return None;
        }
        let index = inner.events.len();
        inner.events.push(LoggedEvent { k: kind.tag(), d: data });
        drop(inner);
        self.wake.notify_waiters();
        Some(index)
    }

    pub fn finish(&self) {
        self.inner.lock().unwrap().finished = true;
        self.wake.notify_waiters();
    }

    /// events from `from` on, capped by size. always returns at least one
    /// event when there is one, however large, so a single big event can't
    /// get stuck behind the cap
    pub fn read(&self, from: usize) -> Slice {
        let inner = self.inner.lock().unwrap();
        let from = from.min(inner.events.len());
        let mut size = 0;
        let mut events = Vec::new();
        for event in &inner.events[from..] {
            let cost = event.d.len() + 32;
            if !events.is_empty() && size + cost > MAX_SLICE_BYTES {
                break;
            }
            size += cost;
            events.push(event.clone());
        }
        let next = from + events.len();
        let done = inner.finished && next == inner.events.len();
        Slice { events, next, done }
    }

    /// waits until there is something at `from`, the generation is over, or
    /// `limit` passes. then reads.
    pub async fn read_waiting(&self, from: usize, limit: Duration) -> Slice {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            // register interest before checking so an event landing between
            // the check and the wait can't be missed
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let slice = self.read(from);
            if !slice.events.is_empty() || slice.done {
                return slice;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return self.read(from);
            }
        }
    }
}

/// where a producer sends what it streams. every event goes in the log, and
/// goes to the live stream too while that is still keeping up. a stream that
/// is closed or backed up is dropped for good rather than waited on, because
/// the log is what the client falls back to, and a stalled connection must
/// never hold up the reply being generated and saved.
pub struct GenSink {
    tx: mpsc::Sender<Result<Event, Infallible>>,
    live: Option<Arc<LiveGeneration>>,
    stream_open: AtomicBool,
}

impl GenSink {
    pub fn channel(live: Option<Arc<LiveGeneration>>) -> (Self, mpsc::Receiver<Result<Event, Infallible>>) {
        let (tx, rx) = mpsc::channel(256);
        (Self { tx, live, stream_open: AtomicBool::new(true) }, rx)
    }

    fn send(&self, kind: Kind, data: String) {
        let index = self.live.as_ref().and_then(|live| live.push(kind, data.clone()));
        let mut event = match kind {
            Kind::Data => Event::default().data(&data),
            Kind::Member => Event::default().event("member").data(&data),
            Kind::Error => Event::default().event("error").data(&data),
            Kind::Done => Event::default().event("done").data(""),
        };
        // the position in the log rides along as the standard SSE id, so a
        // client that loses the stream knows exactly where to resume from
        if let Some(index) = index {
            event = event.id(index.to_string());
        }
        if self.stream_open.load(Ordering::Relaxed) && self.tx.try_send(Ok(event)).is_err() {
            self.stream_open.store(false, Ordering::Relaxed);
        }
    }

    pub fn data(&self, delta: String) {
        self.send(Kind::Data, delta);
    }
    pub fn member(&self, json: String) {
        self.send(Kind::Member, json);
    }
    pub fn error(&self, message: String) {
        self.send(Kind::Error, message);
    }
    pub fn done(&self) {
        self.send(Kind::Done, String::new());
    }
}

impl Drop for GenSink {
    fn drop(&mut self) {
        // every exit of a producer task, including the early returns, ends up
        // here, so a poller is never left waiting on a reply that is over
        if let Some(live) = &self.live {
            live.finish();
        }
    }
}

/// a client-chosen id is only a lookup key, but keep it to a plain shape
pub fn valid_generation_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// registers a log under the id the client sent, if it sent a usable one
pub async fn start(
    state: &crate::state::AppState,
    user_id: i64,
    chat_id: &str,
    generation_id: Option<&str>,
) -> Option<Arc<LiveGeneration>> {
    let id = generation_id.filter(|id| valid_generation_id(id))?;
    let live = LiveGeneration::new(user_id, chat_id.to_string());
    state.live_generations.insert(id.to_string(), live.clone()).await;
    Some(live)
}

/// the id header the frontend sends with a generation request
pub const GENERATION_ID_HEADER: &str = "x-generation-id";

pub fn id_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers.get(GENERATION_ID_HEADER).and_then(|v| v.to_str().ok()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_with(events: &[(Kind, &str)]) -> Arc<LiveGeneration> {
        let live = LiveGeneration::new(1, "chat".into());
        for (kind, data) in events {
            live.push(*kind, data.to_string());
        }
        live
    }

    #[test]
    fn read_returns_events_from_the_index_on() {
        let live = log_with(&[(Kind::Data, "a"), (Kind::Data, "b"), (Kind::Data, "c")]);
        let slice = live.read(1);
        assert_eq!(slice.events.iter().map(|e| e.d.as_str()).collect::<Vec<_>>(), ["b", "c"]);
        assert_eq!(slice.next, 3);
        assert!(!slice.done);
    }

    #[test]
    fn done_only_once_finished_and_everything_was_read() {
        let live = log_with(&[(Kind::Data, "a"), (Kind::Done, "")]);
        assert!(!live.read(0).done);
        live.finish();
        assert!(live.read(0).done);
        let past_end = live.read(99);
        assert!(past_end.events.is_empty());
        assert!(past_end.done);
    }

    #[test]
    fn a_slice_stops_at_the_size_cap_and_the_next_picks_up() {
        let big = "x".repeat(2500);
        let live = log_with(&[(Kind::Data, &big), (Kind::Data, &big), (Kind::Data, &big), (Kind::Data, &big)]);
        live.finish();
        let first = live.read(0);
        assert_eq!(first.events.len(), 2);
        assert!(!first.done);
        let second = live.read(first.next);
        assert_eq!(second.events.len(), 2);
        assert!(second.done);
    }

    #[test]
    fn one_oversized_event_still_comes_through() {
        let huge = "y".repeat(MAX_SLICE_BYTES * 3);
        let live = log_with(&[(Kind::Data, &huge)]);
        assert_eq!(live.read(0).events.len(), 1);
    }

    #[test]
    fn events_after_finish_are_ignored() {
        let live = log_with(&[(Kind::Data, "a")]);
        live.finish();
        live.push(Kind::Data, "late".into());
        assert_eq!(live.read(0).events.len(), 1);
    }

    #[tokio::test]
    async fn waiting_returns_as_soon_as_an_event_lands() {
        let live = LiveGeneration::new(1, "chat".into());
        let writer = live.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            writer.push(Kind::Data, "hi".into());
        });
        let started = std::time::Instant::now();
        let slice = live.read_waiting(0, Duration::from_secs(5)).await;
        assert_eq!(slice.events.len(), 1);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn waiting_gives_up_empty_at_the_limit() {
        let live = LiveGeneration::new(1, "chat".into());
        let slice = live.read_waiting(0, Duration::from_millis(60)).await;
        assert!(slice.events.is_empty());
        assert!(!slice.done);
    }

    #[tokio::test]
    async fn waiting_wakes_when_the_generation_ends() {
        let live = LiveGeneration::new(1, "chat".into());
        let closer = live.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            closer.finish();
        });
        assert!(live.read_waiting(0, Duration::from_secs(5)).await.done);
    }

    #[tokio::test]
    async fn dropping_the_sink_finishes_the_log() {
        let live = LiveGeneration::new(1, "chat".into());
        let (sink, _rx) = GenSink::channel(Some(live.clone()));
        sink.data("a".into());
        assert!(!live.read(0).done);
        drop(sink);
        assert!(live.read(0).done);
    }

    #[tokio::test]
    async fn a_full_or_closed_stream_never_blocks_the_log() {
        let live = LiveGeneration::new(1, "chat".into());
        let (sink, rx) = GenSink::channel(Some(live.clone()));
        drop(rx);
        for i in 0..1000 {
            sink.data(i.to_string());
        }
        assert_eq!(live.read(0).next, 1000.min(live.read(0).events.len()));
        let mut from = 0;
        let mut total = 0;
        loop {
            let slice = live.read(from);
            if slice.events.is_empty() {
                break;
            }
            total += slice.events.len();
            from = slice.next;
        }
        assert_eq!(total, 1000);
    }

    #[test]
    fn push_reports_each_events_position_and_stops_after_finish() {
        let live = LiveGeneration::new(1, "chat".into());
        assert_eq!(live.push(Kind::Data, "a".into()), Some(0));
        assert_eq!(live.push(Kind::Data, "b".into()), Some(1));
        live.finish();
        assert_eq!(live.push(Kind::Data, "c".into()), None);
    }

    #[test]
    fn generation_ids_are_kept_to_a_plain_shape() {
        assert!(valid_generation_id("a1b2c3d4e5f6"));
        assert!(valid_generation_id("abc-DEF_123456"));
        assert!(!valid_generation_id("short"));
        assert!(!valid_generation_id("has spaces in it"));
        assert!(!valid_generation_id("../../etc/passwd"));
        assert!(!valid_generation_id(&"a".repeat(65)));
    }
}
