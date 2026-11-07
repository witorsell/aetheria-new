use gloo_net::http::Request;
use serde::Serialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[derive(Serialize)]
struct LoginRequest<'a> {
    username: &'a str,
    password: &'a str,
}

pub async fn login(username: &str, password: &str) -> Result<(), String> {
    let response = Request::post("/api/login")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&LoginRequest { username, password })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        Ok(())
    } else {
        Err("invalid username or password".to_string())
    }
}

pub async fn register(username: &str, password: &str) -> Result<(), String> {
    let response = Request::post("/api/register")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&LoginRequest { username, password })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if response.ok() {
        Ok(())
    } else if response.status() == 409 {
        Err("username is already taken".to_string())
    } else if response.status() == 403 {
        Err("registration is disabled".to_string())
    } else {
        Err("registration failed".to_string())
    }
}

pub async fn registration_enabled() -> bool {
    match Request::get("/api/registration-status")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
    {
        Ok(resp) if resp.ok() => {
            resp.json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v: serde_json::Value| v.get("enabled").and_then(|e| e.as_bool()))
                .unwrap_or(false)
        }
        _ => false,
    }
}

pub async fn logout() -> Result<(), String> {
    Request::post("/api/logout")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// a mobile browser backgrounded mid-stream can suspend the tab without the
// underlying connection ever erroring out, and a mobile carrier's NAT can kill
// an idle connection outright with neither side seeing a clean close - either
// way the fetch can go silent forever with no error and no more data, leaving
// the caller's "generating"/"loading" flag stuck forever too. this watchdog
// aborts the request if too long passes with nothing at all coming through,
// which routes it into the same on_error path a normal dropped connection
// already takes below instead of hanging. the timer itself is also suspended
// while the tab is hidden (same as any other JS timer), so it doesn't fire
// silently in the background - it fires the moment the tab is foregrounded
// again and real time has already blown past the deadline, which is exactly
// when the user is looking at the stuck UI wondering why it's still spinning.
const IDLE_TIMEOUT_MS: u32 = 90_000;

fn arm_idle_timeout(abort_controller: &web_sys::AbortController) -> gloo_timers::callback::Timeout {
    let abort_controller = abort_controller.clone();
    gloo_timers::callback::Timeout::new(IDLE_TIMEOUT_MS, move || abort_controller.abort())
}

// `_idle_timeout` is reassigned purely to reset the watchdog (each new value's
// Drop cancels the one it replaces) - it's never read directly, which the
// compiler can't tell apart from a genuine dead store.
// how long the live stream may stay silent before the reply is picked up by
// polling instead. the path to the server freezes connections now and then and
// the reply would sit there for half a minute. a model that is just slow to
// answer trips this too, which is harmless: polling shows the same text
const STALL_MS: u32 = 3_000;
// how long one poll is held open waiting for a new event. the Cloudflare
// worker in front of the server gives up on a request after 6s, so stay under
const POLL_WAIT_MS: u32 = 4_000;
// a poll that itself goes quiet is cut off and asked again from the same spot
const POLL_TIMEOUT_MS: u32 = 9_000;
// keep retrying a failing poll this long before calling the connection dropped
const POLL_GIVE_UP_MS: f64 = 60_000.0;
// the log is registered when the request reaches the server, which can lag a
// poll that started early
const POLL_NOT_FOUND_TRIES: u32 = 12;

// how long a repeatable request may take before a second copy is sent alongside
const HEDGE_MS: u32 = 3_000;

/// runs a request that is safe to send twice. if it hasn't answered after
/// HEDGE_MS a second copy goes out on its own connection and whichever answers
/// first wins. the connection a request lands on can freeze for half a minute
/// while a fresh one would have answered at once. an error only counts once
/// both copies have failed.
pub async fn hedged<T, F, Fut>(make: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    use futures_util::future::{select, Either};
    let first = Box::pin(make());
    match select(first, gloo_timers::future::TimeoutFuture::new(HEDGE_MS)).await {
        Either::Left((answer, _)) => answer,
        Either::Right((_, first)) => match select(first, Box::pin(make())).await {
            Either::Left((Ok(value), _)) | Either::Right((Ok(value), _)) => Ok(value),
            Either::Left((Err(e), second)) => second.await.map_err(|_| e),
            Either::Right((Err(e), first)) => first.await.map_err(|_| e),
        },
    }
}

/// a repeatable GET that tries again on failure instead of giving up on the
/// first bad answer. each try is itself hedged. the path to the server drops
/// a request now and then and a retry a moment later nearly always works. a
/// logged-out session is not retried, since asking again won't change it
pub async fn resilient<T, F, Fut>(make: F) -> Result<T, String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    const TRIES: u32 = 4;
    let mut last = String::new();
    for attempt in 0..TRIES {
        if attempt > 0 {
            gloo_timers::future::TimeoutFuture::new(400 * attempt).await;
        }
        match hedged(&make).await {
            Ok(value) => return Ok(value),
            Err(e) if e == SESSION_EXPIRED_ERROR => return Err(e),
            Err(e) => last = e,
        }
    }
    Err(last)
}

enum Raced<T> {
    Ready(T),
    Stalled,
}

/// waits for `fut`, or gives up on it after `stall_ms`. no limit when `None`
async fn race_stall<F: std::future::Future + Unpin>(fut: F, stall_ms: Option<u32>) -> Raced<F::Output> {
    let Some(ms) = stall_ms else { return Raced::Ready(fut.await) };
    match futures_util::future::select(fut, gloo_timers::future::TimeoutFuture::new(ms)).await {
        futures_util::future::Either::Left((out, _)) => Raced::Ready(out),
        futures_util::future::Either::Right(_) => Raced::Stalled,
    }
}

/// hands one event to the right callback. true when it was the end of the reply
fn dispatch_event(
    kind: SseEventKind,
    data: Option<String>,
    on_delta: &mut impl FnMut(String),
    on_member: &mut impl FnMut(String, String),
    on_error: &mut impl FnMut(String),
) -> bool {
    match kind {
        SseEventKind::Done => return true,
        SseEventKind::Error => {
            if let Some(data) = data {
                on_error(data);
            }
        }
        SseEventKind::Member => {
            if let Some(data) = data {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                    let character_id = parsed.get("character_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    let name = parsed.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                    if let (Some(character_id), Some(name)) = (character_id, name) {
                        on_member(character_id.to_string(), name.to_string());
                    }
                }
            }
        }
        SseEventKind::Data => {
            if let Some(data) = data {
                on_delta(data);
            }
        }
    }
    false
}

/// reads every complete event out of `buffer`. `resume_from` follows the
/// server's own position for the last event seen, so a fallback to polling
/// asks for exactly what is missing. returns true once `event: done` was seen:
/// the caller stops reading there, since the trailing EOF can take several
/// seconds to show up on the phone and the reply looks frozen for that wait.
fn drain_events(
    buffer: &mut String,
    resume_from: &mut usize,
    on_delta: &mut impl FnMut(String),
    on_member: &mut impl FnMut(String, String),
    on_error: &mut impl FnMut(String),
) -> bool {
    while let Some(pos) = buffer.find("\n\n") {
        let event_block = buffer[..pos].to_string();
        buffer.drain(..pos + 2);

        let (kind, data) = parse_event_block(&event_block);
        if let Some(id) = super::stream_resume::sse_event_id(&event_block) {
            *resume_from = id + 1;
        }
        if dispatch_event(kind, data, on_delta, on_member, on_error) {
            return true;
        }
    }
    false
}

enum Polled {
    Events(super::stream_resume::PollBody),
    NotFound,
    Unauthorized,
}

async fn poll_once(url: &str) -> Result<Polled, String> {
    let abort = web_sys::AbortController::new().map_err(|e| format!("{e:?}"))?;
    let abort_on_timeout = abort.clone();
    let _cutoff = gloo_timers::callback::Timeout::new(POLL_TIMEOUT_MS, move || abort_on_timeout.abort());
    let response = Request::get(url)
        .credentials(web_sys::RequestCredentials::Include)
        .abort_signal(Some(&abort.signal()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match response.status() {
        401 => return Ok(Polled::Unauthorized),
        404 => return Ok(Polled::NotFound),
        status if !(200..300).contains(&status) => return Err(format!("poll answered {status}")),
        _ => {}
    }
    let text = response.text().await.map_err(|e| e.to_string())?;
    super::stream_resume::parse_poll_body(&text).map(Polled::Events)
}

/// reads the rest of a reply from the server's event log, in small answers on
/// fresh connections, starting at `from`. each answer is far below the size at
/// which the path freezes a connection. `not_found_tries` is how many times a
/// missing log is retried before giving up, and `missing_is_error` says whether
/// giving up is a failure or just a reply that needs no more reading.
#[allow(clippy::too_many_arguments)]
async fn resume_by_polling(
    base: &str,
    generation_id: &str,
    mut from: usize,
    not_found_tries: u32,
    missing_is_error: bool,
    on_delta: &mut impl FnMut(String),
    on_member: &mut impl FnMut(String, String),
    on_error: &mut impl FnMut(String),
) {
    let mut not_found = 0;
    let mut failing_since: Option<f64> = None;
    loop {
        let url = super::stream_resume::poll_url(base, generation_id, from, POLL_WAIT_MS);
        match poll_once(&url).await {
            Ok(Polled::Events(body)) => {
                failing_since = None;
                not_found = 0;
                for event in body.events {
                    let kind = match event.k.as_str() {
                        "error" => SseEventKind::Error,
                        "member" => SseEventKind::Member,
                        "done" => SseEventKind::Done,
                        _ => SseEventKind::Data,
                    };
                    if dispatch_event(kind, Some(event.d), on_delta, on_member, on_error) {
                        return;
                    }
                }
                from = body.next;
                if body.done {
                    return;
                }
            }
            Ok(Polled::Unauthorized) => {
                on_error(SESSION_EXPIRED_ERROR.to_string());
                return;
            }
            Ok(Polled::NotFound) => {
                not_found += 1;
                if not_found >= not_found_tries {
                    if missing_is_error {
                        on_error("stream read failed: the reply was lost".to_string());
                    }
                    return;
                }
                gloo_timers::future::TimeoutFuture::new(1_000).await;
            }
            Err(e) => {
                let now = js_sys::Date::now();
                let since = *failing_since.get_or_insert(now);
                if now - since > POLL_GIVE_UP_MS {
                    on_error(format!("stream read failed: {e}"));
                    return;
                }
                gloo_timers::future::TimeoutFuture::new(1_000).await;
            }
        }
    }
}

// `_idle_timeout` is reassigned purely to reset the watchdog (each new value's
// Drop cancels the one it replaces) - it's never read directly, which the
// compiler can't tell apart from a genuine dead store.
#[allow(unused_assignments)]
pub async fn stream_post(
    url: &str,
    body: Option<String>,
    mut on_delta: impl FnMut(String),
    mut on_member: impl FnMut(String, String),
    mut on_error: impl FnMut(String),
) -> Result<(), String> {
    let abort_controller = web_sys::AbortController::new().map_err(|e| format!("{e:?}"))?;
    let mut _idle_timeout = arm_idle_timeout(&abort_controller);

    // the server keeps a log of this reply under an id we pick, so if the live
    // stream freezes the rest can be fetched from it. only chat routes have one
    let poll_base = super::stream_resume::chat_base(url);
    let generation_id = super::stream_resume::generation_id_from(
        js_sys::Date::now() as u64,
        (js_sys::Math::random() * 1e15) as u64,
        (js_sys::Math::random() * 1e15) as u64,
    );
    let stall_ms = poll_base.as_ref().map(|_| STALL_MS);
    // the position in the server's log of the last event the stream delivered
    let mut resume_from: usize = 0;

    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_credentials(web_sys::RequestCredentials::Include);
    init.set_signal(Some(&abort_controller.signal()));
    if let Some(body) = &body {
        init.set_body(&JsValue::from_str(body));
    }
    let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    if poll_base.is_some() {
        headers.set("X-Generation-Id", &generation_id).map_err(|e| format!("{e:?}"))?;
    }
    init.set_headers(&headers);

    let request =
        web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or("no window")?;

    macro_rules! resume {
        ($tries:expr, $missing_is_error:expr) => {{
            if let Some(base) = &poll_base {
                resume_by_polling(base, &generation_id, resume_from, $tries, $missing_is_error, &mut on_delta, &mut on_member, &mut on_error).await;
                // the live request may still be hanging around
                abort_controller.abort();
                return Ok(());
            }
        }};
    }

    let fetched = race_stall(JsFuture::from(window.fetch_with_request(&request)), stall_ms).await;
    let response_value = match fetched {
        Raced::Ready(Ok(value)) => value,
        Raced::Ready(Err(e)) => {
            resume!(POLL_NOT_FOUND_TRIES, true);
            return Err(format!("{e:?}"));
        }
        Raced::Stalled => {
            resume!(POLL_NOT_FOUND_TRIES, true);
            return Err("request stalled".to_string());
        }
    };
    _idle_timeout = arm_idle_timeout(&abort_controller);
    let response: web_sys::Response = response_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if response.status() == 401 {
        on_error(SESSION_EXPIRED_ERROR.to_string());
        return Ok(());
    }

    if !response.ok() {
        // a non-2xx body is a JSON {code, message} error object (see
        // server::error::ApiError), not an SSE stream - feeding it to the
        // line parser below would just never find a "\n\n" event boundary
        // and silently return Ok(()) once the reader hits EOF, as if
        // generation had completed normally with no reply
        let status = response.status();
        let message = match JsFuture::from(response.text().map_err(|e| format!("{e:?}"))?).await {
            Ok(text_value) => {
                let text = text_value.as_string().unwrap_or_default();
                serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
                    .filter(|m| !m.is_empty())
                    .unwrap_or(text)
            }
            Err(_) => String::new(),
        };
        let message = if message.trim().is_empty() {
            format!("request failed with status {status}")
        } else {
            message
        };
        on_error(message);
        return Ok(());
    }

    let body = response.body().ok_or("no response body")?;
    let reader: web_sys::ReadableStreamDefaultReader =
        body.get_reader().dyn_into().map_err(|e| format!("{e:?}"))?;

    let decoder = web_sys::TextDecoder::new().map_err(|e| format!("{e:?}"))?;
    let mut buffer = String::new();

    loop {
        let chunk_value = match race_stall(JsFuture::from(reader.read()), stall_ms).await {
            Raced::Ready(Ok(v)) => v,
            Raced::Ready(Err(e)) => {
                // a dropped connection mid-stream used to just break silently
                // here and return Ok(()), same dead-end as an unhandled
                // non-2xx response above - the caller never learns anything
                // went wrong. with a log to fall back on, ask it instead
                resume!(POLL_NOT_FOUND_TRIES, true);
                on_error(format!("stream read failed: {e:?}"));
                break;
            }
            Raced::Stalled => {
                resume!(POLL_NOT_FOUND_TRIES, true);
                on_error("stream read failed: stalled".to_string());
                break;
            }
        };
        _idle_timeout = arm_idle_timeout(&abort_controller);
        let done = js_sys::Reflect::get(&chunk_value, &JsValue::from_str("done"))
            .map_err(|e| format!("{e:?}"))?
            .as_bool()
            .unwrap_or(true);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&chunk_value, &JsValue::from_str("value"))
            .map_err(|e| format!("{e:?}"))?;
        let array: js_sys::Uint8Array = value.dyn_into().map_err(|e| format!("{e:?}"))?;
        let mut options = web_sys::TextDecodeOptions::new();
        options.stream(true);
        let text = decoder
            .decode_with_buffer_source_and_options(&array, &options)
            .map_err(|e| format!("{e:?}"))?;
        buffer.push_str(&text);
        if drain_events(&mut buffer, &mut resume_from, &mut on_delta, &mut on_member, &mut on_error) {
            let _ = abort_controller.abort();
            return Ok(());
        }
    }

    // flush any bytes the streaming decoder held back mid-character on the
    // final chunk - decode_with_buffer_source_and_options(stream: true)
    // intentionally withholds an incomplete trailing UTF-8 sequence in case
    // more bytes are coming, so a plain decode() call is needed once the
    // stream is actually done to emit it (or drop it if input truly ended
    // mid-character, which is the correct behavior at EOF)
    if let Ok(tail) = decoder.decode() {
        if !tail.is_empty() {
            buffer.push_str(&tail);
            if drain_events(&mut buffer, &mut resume_from, &mut on_delta, &mut on_member, &mut on_error) {
                return Ok(());
            }
        }
    }

    // the stream closed without ever saying `done`. the server may have
    // stopped feeding it (it never waits on a backed-up connection) or the
    // connection may have been cut, so ask the log whether more is coming. a
    // reply with no log behind it just ends here, as it always did
    resume!(3, false);

    Ok(())
}

pub const SESSION_EXPIRED_ERROR: &str = "session expired, please log in again";

/// the chat banner is one line under the transcript. a fetch rejection's
/// debug string includes the whole stack, and that block was tall enough
/// to be the only thing on screen.
pub fn brief_net_error(err: impl AsRef<str>) -> String {
    let err = err.as_ref();
    if err.contains("Failed to fetch")
        || err.contains("NetworkError")
        || err.contains("Load failed")
        || err.contains("aborted")
        || err.contains("stream read failed")
    {
        return "connection dropped".to_string();
    }
    err.lines().next().unwrap_or(err).chars().take(180).collect()
}

#[derive(PartialEq, Debug)]
pub(crate) enum SseEventKind {
    Data,
    Error,
    Member,
    /// server has finished the reply. the bytes are all here. the connection
    /// itself can stay open for seconds after this, and waiting on that
    /// close is the pause at the end of a stream.
    Done,
}

pub(crate) fn parse_event_block(event_block: &str) -> (SseEventKind, Option<String>) {
    let event_name = event_block
        .lines()
        .find_map(|line| line.strip_prefix("event:"))
        .map(|name| name.trim());

    let kind = match event_name {
        Some("error") => SseEventKind::Error,
        Some("member") => SseEventKind::Member,
        Some("done") => SseEventKind::Done,
        _ => SseEventKind::Data,
    };

    let data_lines: Vec<&str> = event_block
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| data.strip_prefix(' ').unwrap_or(data))
        .collect();

    let combined = if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    };
    (kind, combined)
}

pub fn download_text_file(filename: &str, mime_type: &str, contents: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;

    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(contents));
    let blob_options = web_sys::BlobPropertyBag::new();
    blob_options.set_type(mime_type);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &blob_options).map_err(|_| "failed to build file".to_string())?;

    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| "failed to create download link".to_string())?;

    let anchor = document
        .create_element("a")
        .map_err(|_| "failed to create download link".to_string())?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "failed to create download link".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();

    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

const TEXT_SPEED_KEY: &str = "aetheria_text_speed";

pub fn get_text_speed() -> u32 {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(TEXT_SPEED_KEY).ok().flatten())
        .and_then(|value| value.parse().ok())
        .unwrap_or(40)
}

pub fn set_text_speed(speed: u32) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(TEXT_SPEED_KEY, &speed.to_string());
    }
}

const SUBSCRIPTION_ONLY_MODELS_KEY: &str = "aetheria_subscription_only_models";

pub fn get_subscription_only_models() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item(SUBSCRIPTION_ONLY_MODELS_KEY).ok().flatten())
        .map(|value| value == "true")
        .unwrap_or(false)
}

pub fn set_subscription_only_models(value: bool) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(SUBSCRIPTION_ONLY_MODELS_KEY, if value { "true" } else { "false" });
    }
}

#[cfg(test)]
mod tests {
    use super::parse_event_block;
    use super::SseEventKind;

    #[test]
    fn strips_exactly_one_leading_space_preserving_meaningful_whitespace() {
        let (kind, data) = parse_event_block("data:  world");
        assert_eq!(kind, SseEventKind::Data);
        assert_eq!(data, Some(" world".to_string()));
    }

    #[test]
    fn data_with_no_leading_space_is_unchanged() {
        let (_, data) = parse_event_block("data:hello");
        assert_eq!(data, Some("hello".to_string()));
    }

    #[test]
    fn joins_multiple_data_lines_in_one_block_with_newline() {
        let (kind, data) = parse_event_block("data: line one\ndata: line two");
        assert_eq!(kind, SseEventKind::Data);
        assert_eq!(data, Some("line one\nline two".to_string()));
    }

    #[test]
    fn routes_error_event_blocks_to_error_with_data_joined_the_same_way() {
        let (kind, data) = parse_event_block("event: error\ndata: boom");
        assert_eq!(kind, SseEventKind::Error);
        assert_eq!(data, Some("boom".to_string()));
    }

    #[test]
    fn a_fetch_rejection_does_not_keep_its_stack() {
        let shown = super::brief_net_error(
            "JsValue(TypeError: Failed to fetch\nTypeError: Failed to fetch\n    at window.fetch)",
        );
        assert_eq!(shown, "connection dropped");
    }

    #[test]
    fn block_with_no_data_line_yields_none() {
        let (kind, data) = parse_event_block("event: ping");
        assert_eq!(kind, SseEventKind::Data);
        assert_eq!(data, None);
    }

    #[test]
    fn done_event_is_not_appended_to_the_reply() {
        let (kind, data) = parse_event_block("event: done\ndata:");
        assert_eq!(kind, SseEventKind::Done);
        assert_eq!(data, Some(String::new()));
    }
}

pub async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = Request::get(url)
        .credentials(web_sys::RequestCredentials::Include)
        .send().await.map_err(|_| "couldn't connect, please try again".to_string())?;
    if response.status() == 401 {
        return Err(SESSION_EXPIRED_ERROR.to_string());
    }
    if !response.ok() {
        return Err(format!("request failed ({})", response.status()));
    }
    response.json().await.map_err(|_| "couldn't read the response, please try again".to_string())
}
