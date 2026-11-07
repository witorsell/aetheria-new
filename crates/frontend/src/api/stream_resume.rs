//! the plain parts of resuming a reply by polling, kept free of the browser so
//! they can be tested natively. see server::live_generation for why a reply
//! stream needs a way back: the live connection can freeze, and the server
//! keeps a numbered log of the reply that can be read in small pieces.

use serde::Deserialize;

/// `/api/chats/{id}/generate` -> `/api/chats/{id}`. anything that isn't a
/// chat route has no event log behind it and gets no resume
pub fn chat_base(url: &str) -> Option<String> {
    let path = url.split('?').next()?;
    let mut parts = path.trim_start_matches('/').split('/');
    if parts.next()? != "api" || parts.next()? != "chats" {
        return None;
    }
    let id = parts.next().filter(|id| !id.is_empty())?;
    Some(format!("/api/chats/{id}"))
}

/// the SSE `id:` of an event block: its position in the server's log
pub fn sse_event_id(block: &str) -> Option<usize> {
    block
        .lines()
        .find_map(|line| line.strip_prefix("id:"))
        .and_then(|id| id.trim().parse().ok())
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct PollEvent {
    pub k: String,
    pub d: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct PollBody {
    pub events: Vec<PollEvent>,
    pub next: usize,
    pub done: bool,
}

pub fn parse_poll_body(text: &str) -> Result<PollBody, String> {
    serde_json::from_str(text).map_err(|e| format!("bad poll answer: {e}"))
}

pub fn poll_url(base: &str, generation_id: &str, from: usize, wait_ms: u32) -> String {
    format!("{base}/generation/{generation_id}/events?from={from}&wait={wait_ms}")
}

/// a plain id for one request, safe in a header and a url
pub fn generation_id_from(now_ms: u64, random_a: u64, random_b: u64) -> String {
    format!("{now_ms:x}{random_a:x}{random_b:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_routes_give_their_base() {
        assert_eq!(chat_base("/api/chats/abc-123/generate"), Some("/api/chats/abc-123".into()));
        assert_eq!(chat_base("/api/chats/abc-123/regenerate?parent_id=x"), Some("/api/chats/abc-123".into()));
        assert_eq!(chat_base("/api/chats/abc-123/continue"), Some("/api/chats/abc-123".into()));
    }

    #[test]
    fn other_routes_give_nothing() {
        assert_eq!(chat_base("/api/characters/abc/generate"), None);
        assert_eq!(chat_base("/api/chats/"), None);
        assert_eq!(chat_base("/api/chats"), None);
        assert_eq!(chat_base("/somewhere/else"), None);
        assert_eq!(chat_base(""), None);
    }

    #[test]
    fn the_sse_id_is_read_from_its_line() {
        assert_eq!(sse_event_id("id: 7\ndata: hello"), Some(7));
        assert_eq!(sse_event_id("event: done\nid:12\ndata:"), Some(12));
        assert_eq!(sse_event_id("data: hello"), None);
        assert_eq!(sse_event_id(": keep-alive"), None);
        assert_eq!(sse_event_id("id: nope\ndata: x"), None);
    }

    #[test]
    fn a_poll_answer_parses() {
        let body = parse_poll_body(r#"{"events":[{"k":"data","d":"Hel"},{"k":"done","d":""}],"next":2,"done":true}"#).unwrap();
        assert_eq!(body.next, 2);
        assert!(body.done);
        assert_eq!(body.events[0], PollEvent { k: "data".into(), d: "Hel".into() });
        assert_eq!(body.events[1].k, "done");
    }

    #[test]
    fn text_with_newlines_and_quotes_survives_a_poll_answer() {
        let body = parse_poll_body(r#"{"events":[{"k":"data","d":"a\n\"b\""}],"next":1,"done":false}"#).unwrap();
        assert_eq!(body.events[0].d, "a\n\"b\"");
    }

    #[test]
    fn a_garbled_answer_is_an_error_not_a_panic() {
        assert!(parse_poll_body("<html>502</html>").is_err());
        assert!(parse_poll_body("").is_err());
        assert!(parse_poll_body(r#"{"events":[]}"#).is_err());
    }

    #[test]
    fn the_poll_url_names_the_chat_generation_and_position() {
        assert_eq!(
            poll_url("/api/chats/c1", "gen1", 5, 8000),
            "/api/chats/c1/generation/gen1/events?from=5&wait=8000"
        );
    }

    #[test]
    fn a_generation_id_fits_what_the_server_accepts() {
        let id = generation_id_from(1_794_000_000_000, 123_456_789_012_345, 987_654_321_098_765);
        assert!((8..=64).contains(&id.len()), "{id}");
        assert!(id.bytes().all(|b| b.is_ascii_alphanumeric()));
    }
}
