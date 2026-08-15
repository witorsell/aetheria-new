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

pub async fn stream_post(
    url: &str,
    body: Option<String>,
    mut on_delta: impl FnMut(String),
    mut on_member: impl FnMut(String, String),
    mut on_error: impl FnMut(String),
) -> Result<(), String> {
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_credentials(web_sys::RequestCredentials::Include);
    if let Some(body) = &body {
        init.set_body(&JsValue::from_str(body));
    }
    let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    init.set_headers(&headers);

    let request =
        web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or("no window")?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
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

    fn drain_events(
        buffer: &mut String,
        on_delta: &mut impl FnMut(String),
        on_member: &mut impl FnMut(String, String),
        on_error: &mut impl FnMut(String),
    ) {
        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer.drain(..pos + 2);

            let (kind, data) = parse_event_block(&event_block);
            let Some(data) = data else { continue };
            match kind {
                SseEventKind::Error => on_error(data),
                SseEventKind::Member => {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&data) {
                        let character_id = parsed.get("character_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                        let name = parsed.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
                        if let (Some(character_id), Some(name)) = (character_id, name) {
                            on_member(character_id.to_string(), name.to_string());
                        }
                    }
                }
                SseEventKind::Data => on_delta(data),
            }
        }
    }

    loop {
        let chunk_value = match JsFuture::from(reader.read()).await {
            Ok(v) => v,
            Err(e) => {
                // a dropped connection mid-stream used to just break silently
                // here and return Ok(()), same dead-end as an unhandled
                // non-2xx response above - the caller never learns anything
                // went wrong
                on_error(format!("stream read failed: {e:?}"));
                break;
            }
        };
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
        drain_events(&mut buffer, &mut on_delta, &mut on_member, &mut on_error);
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
            drain_events(&mut buffer, &mut on_delta, &mut on_member, &mut on_error);
        }
    }

    Ok(())
}

pub const SESSION_EXPIRED_ERROR: &str = "session expired, please log in again";

#[derive(PartialEq, Debug)]
pub(crate) enum SseEventKind {
    Data,
    Error,
    Member,
}

pub(crate) fn parse_event_block(event_block: &str) -> (SseEventKind, Option<String>) {
    let event_name = event_block
        .lines()
        .find_map(|line| line.strip_prefix("event:"))
        .map(|name| name.trim());

    let kind = match event_name {
        Some("error") => SseEventKind::Error,
        Some("member") => SseEventKind::Member,
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
    fn block_with_no_data_line_yields_none() {
        let (kind, data) = parse_event_block("event: ping");
        assert_eq!(kind, SseEventKind::Data);
        assert_eq!(data, None);
    }
}
