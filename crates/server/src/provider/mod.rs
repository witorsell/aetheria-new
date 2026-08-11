pub mod prompt;
pub mod regex_engine;
pub mod anthropic;
pub mod novel;
pub mod gemini;
pub mod horde;

pub use anthropic::AnthropicProvider;
pub use novel::NovelProvider;
pub use gemini::GeminiProvider;
pub use horde::HordeProvider;
use futures_util::StreamExt;
use prompt::ChatMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum ProviderError {
    Request(reqwest::Error),
    Status(u16, String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Request(e) => write!(f, "request failed: {e}"),
            ProviderError::Status(code, body) => write!(f, "provider returned {code}: {body}"),
        }
    }
}

// 0 on top_k/frequency_penalty/presence_penalty/max_tokens = disabled,
// dropped from the request (some providers reject explicit top_k: 0)
#[derive(Clone)]
pub struct SamplingParams {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub frequency_penalty: f64,
    pub presence_penalty: f64,
    pub max_tokens: i64,
    // NanoGPT/OpenRouter reasoning_effort: low/medium/high, "" to omit.
    // only OpenAIProvider wires this up
    pub reasoning_effort: String,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            top_k: 0,
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            max_tokens: 0,
            reasoning_effort: String::new(),
        }
    }
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}
fn is_empty_str(v: &str) -> bool {
    v.is_empty()
}

// anthropic/gemini want a raw token count, not the low/medium/high label.
// None if unset/unrecognized = thinking off
pub fn reasoning_effort_to_budget_tokens(effort: &str) -> Option<i64> {
    match effort {
        "low" => Some(4096),
        "medium" => Some(10000),
        "high" => Some(24000),
        _ => None,
    }
}

#[derive(Serialize)]
struct CompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    top_k: i64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    frequency_penalty: f64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    presence_penalty: f64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    max_tokens: i64,
    #[serde(skip_serializing_if = "is_empty_str")]
    reasoning_effort: String,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
    #[serde(alias = "reasoning_content")]
    reasoning: Option<String>,
}

// decode stream chunk, buffering partial utf8 sequences
fn decode_utf8_chunk(pending: &mut Vec<u8>, chunk: &[u8]) -> String {
    pending.extend_from_slice(chunk);
    let mut decoded = String::new();

    loop {
        match std::str::from_utf8(pending) {
            Ok(s) => {
                decoded.push_str(s);
                pending.clear();
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to > 0 {
                    decoded.push_str(
                        std::str::from_utf8(&pending[..valid_up_to])
                            .expect("valid_up_to guarantees this prefix is valid UTF-8"),
                    );
                }

                match e.error_len() {
                    Some(invalid_len) => {
                        // genuinely invalid bytes, not just incomplete. drop them,
                        // substitute the replacement character, and keep decoding
                        // whatever follows in the same buffer.
                        decoded.push('\u{fffd}');
                        pending.drain(..valid_up_to + invalid_len);
                    }
                    None => {
                        // the tail is a real, still-incomplete character. hold the
                        // undecoded bytes back for the next chunk and stop for now.
                        pending.drain(..valid_up_to);
                        break;
                    }
                }
            }
        }
    }

    decoded
}

use async_trait::async_trait;
use std::pin::Pin;

#[async_trait]
pub trait ModelProvider: Send + Sync {
    async fn stream_completion(
        &self,
        base_url: String,
        api_key: String,
        model: String,
        messages: Vec<ChatMessage>,
        sampling: SamplingParams,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<String, ProviderError>> + Send>>;
}

pub struct OpenAIProvider;

#[async_trait]
impl ModelProvider for OpenAIProvider {
    async fn stream_completion(
        &self,
        base_url: String,
        api_key: String,
        model: String,
        messages: Vec<ChatMessage>,
        sampling: SamplingParams,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<String, ProviderError>> + Send>> {
        Box::pin(async_stream::stream! {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{base_url}/chat/completions"))
            .bearer_auth(api_key)
            .json(&CompletionRequest {
                model, messages, stream: true,
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: sampling.top_k,
                frequency_penalty: sampling.frequency_penalty,
                presence_penalty: sampling.presence_penalty,
                max_tokens: sampling.max_tokens,
                reasoning_effort: sampling.reasoning_effort,
            })
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                yield Err(ProviderError::Request(e));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            yield Err(ProviderError::Status(status, body));
            return;
        }

        let mut byte_stream = response.bytes_stream();
        let mut pending_bytes: Vec<u8> = Vec::new();
        let mut buffer = String::new();
        // some providers (nanogpt/openrouter's GLM thinking models etc) send
        // reasoning as its own delta.reasoning chunks instead of <think> tags.
        // this tracks if we're mid fake-think-block so we can open/close it,
        // matches old aetheria's format. lives outside the loop cuz a
        // reasoning run can span more than one read
        let mut in_reasoning = false;

        while let Some(chunk) = byte_stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    yield Err(ProviderError::Request(e));
                    return;
                }
            };
            buffer.push_str(&decode_utf8_chunk(&mut pending_bytes, &chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);

                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        if in_reasoning {
                            yield Ok("</think>".to_string());
                        }
                        return;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(choice) = parsed.choices.first() {
                            if let Some(reasoning) = &choice.delta.reasoning {
                                if !in_reasoning {
                                    yield Ok("<think>".to_string());
                                    in_reasoning = true;
                                }
                                yield Ok(reasoning.clone());
                            }
                            if let Some(content) = &choice.delta.content {
                                if in_reasoning {
                                    yield Ok("</think>".to_string());
                                    in_reasoning = false;
                                }
                                yield Ok(content.clone());
                            }
                        }
                    }
                }
            }
        }

        // stream ended (upstream closed the connection) without ever sending
        // [DONE] or a content chunk after reasoning started, e.g. an error cut
        // things short. close the tag so the frontend doesn't end up stuck
        // treating the rest of the message as still-thinking.
        if in_reasoning {
            yield Ok("</think>".to_string());
        }
    })
    }
}

#[cfg(test)]
mod tests {
    use super::decode_utf8_chunk;

    // feeds chunks through decode_utf8_chunk one at a time, like
    // stream_completion consuming bytes_stream()
    fn decode_utf8_chunks(chunks: &[&[u8]]) -> String {
        let mut pending = Vec::new();
        let mut text = String::new();
        for chunk in chunks {
            text.push_str(&decode_utf8_chunk(&mut pending, chunk));
        }
        text
    }

    #[test]
    fn reassembles_a_two_byte_character_split_across_chunk_boundary() {
        // 'é' encodes as the two bytes 0xC3 0xA9. split right between them so
        // neither chunk holds a complete character on its own.
        let full = "caf\u{e9}".as_bytes(); // "café"
        let (first, second) = full.split_at(full.len() - 1);

        let decoded = decode_utf8_chunks(&[first, second]);

        assert_eq!(decoded, "caf\u{e9}");
        assert!(!decoded.contains('\u{fffd}'), "must not contain the replacement character");
    }

    #[test]
    fn reassembles_a_four_byte_emoji_split_across_three_chunks() {
        // the party popper emoji is a 4-byte uTF-8 sequence. split it into three
        // pieces so the fix has to hold back partial bytes across two boundaries.
        let emoji = "\u{1f389}".as_bytes();
        assert_eq!(emoji.len(), 4);

        let decoded = decode_utf8_chunks(&[&emoji[..1], &emoji[1..3], &emoji[3..]]);

        assert_eq!(decoded, "\u{1f389}");
        assert!(!decoded.contains('\u{fffd}'), "must not contain the replacement character");
    }

    #[test]
    fn handles_sse_style_chunks_with_a_split_character_mid_line() {
        // mimics an SSE "data: ..." line whose JSON content is split mid multi-byte
        // character across two network chunks, followed by the closing newline.
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"caf\u{e9}\"}}]}\n";
        let bytes = line.as_bytes();
        let split_at = bytes.len() - 2; // splits inside the 2-byte 'é'
        let (first, second) = bytes.split_at(split_at);

        let decoded = decode_utf8_chunks(&[first, second]);

        assert_eq!(decoded, line);
        assert!(!decoded.contains('\u{fffd}'));
    }

    #[test]
    fn skips_a_genuinely_invalid_byte_instead_of_stalling_forever() {
        // 0x80 is a lone continuation byte with no lead byte: not an incomplete
        // character waiting on more bytes, it is invalid outright and will never
        // become valid no matter what arrives next. a naive fix that only checks
        // `valid_up_to()` would buffer it forever and every later chunk would be
        // stuck behind it, since the invalid byte never leaves `pending`.
        let invalid_chunk: &[u8] = &[0x80];
        let later_chunk_one = "hello".as_bytes();
        let later_chunk_two = " world".as_bytes();

        let decoded = decode_utf8_chunks(&[invalid_chunk, later_chunk_one, later_chunk_two]);

        assert!(
            decoded.contains('\u{fffd}'),
            "the invalid byte should be replaced, not silently dropped: {decoded:?}"
        );
        assert!(
            decoded.contains("hello world"),
            "valid content arriving after the invalid byte must still be decoded, the stream must not stall: {decoded:?}"
        );
    }
}

// reasoning <think> synthesis tests against stream_completion itself, via
// a real local mock HTTP server instead of a fake
#[cfg(test)]
mod streaming_tests {
    use super::*;
    use axum::{Router, body::Body, response::Response, routing::post};
    use tokio::net::TcpListener;

    // one-shot mock provider on a random port, POST /chat/completions
    // always returns body verbatim
    async fn spawn_mock_provider(body: &'static str) -> String {
        let app = Router::new().route(
            "/chat/completions",
            post(move || async move {
                Response::builder()
                    .status(200)
                    .header("content-type", "text/event-stream")
                    .body(Body::from(body))
                    .unwrap()
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    async fn collect_stream(base_url: String) -> String {
        let provider = OpenAIProvider;
        let stream = provider.stream_completion(base_url, "key".to_string(), "model".to_string(), vec![], SamplingParams::default()).await;
        let pieces: Vec<String> =
            stream.map(|r| r.expect("stream should not error")).collect().await;
        pieces.concat()
    }

    #[tokio::test]
    async fn wraps_separate_reasoning_deltas_in_one_think_block_before_content() {
        // real shape captured from NanoGPT/zai-org/glm-5.2:thinking: reasoning
        // arrives as its own run of `delta.reasoning` chunks, entirely before
        // any `delta.content` chunk, with no `<think>` tags in the text itself.
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning\":\"1.  Analyze the request.\\n\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"reasoning\":\" 2. Say hi back.\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\", it's nice to meet you!\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
            "data: [DONE]\n",
        );
        let base_url = spawn_mock_provider(body).await;

        let full = collect_stream(base_url).await;

        assert_eq!(
            full,
            "<think>1.  Analyze the request.\n 2. Say hi back.</think>Hi, it's nice to meet you!"
        );

        let (visible, thought) = crate::reasoning::extract_thinking(&full);
        assert_eq!(visible, "Hi, it's nice to meet you!");
        assert_eq!(thought.as_deref(), Some("1.  Analyze the request.\n 2. Say hi back."));
    }

    #[tokio::test]
    async fn leaves_a_content_only_stream_completely_unwrapped() {
        // the common case: a model that never sends `reasoning` at all. no
        // <think> tags should appear anywhere, output must be byte-identical
        // to what the old content-only code produced.
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n",
            "data: [DONE]\n",
        );
        let base_url = spawn_mock_provider(body).await;

        let full = collect_stream(base_url).await;

        assert_eq!(full, "Hello there");
    }

    #[tokio::test]
    async fn closes_an_unterminated_think_block_if_the_stream_ends_mid_reasoning() {
        // stream cuts off with no [DONE] and no content chunk ever arriving,
        // e.g. an upstream error right after reasoning starts. the frontend's
        // extract_thinking must not be left seeing an unterminated <think>
        // "forever" (i.e. no </think> anywhere in the reply).
        let body = "data: {\"choices\":[{\"delta\":{\"reasoning\":\"still pondering\"}}]}\n";
        let base_url = spawn_mock_provider(body).await;

        let full = collect_stream(base_url).await;

        assert_eq!(full, "<think>still pondering</think>");
    }

    #[tokio::test]
    async fn closes_an_unterminated_think_block_at_an_explicit_done_with_no_content() {
        // same as above, but the reasoning-only stream properly terminates with
        // [DONE] instead of the connection just dropping.
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"reasoning\":\"thinking it through\"}}]}\n",
            "data: [DONE]\n",
        );
        let base_url = spawn_mock_provider(body).await;

        let full = collect_stream(base_url).await;

        assert_eq!(full, "<think>thinking it through</think>");
    }
}
