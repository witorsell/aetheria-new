use super::{ModelProvider, ProviderError};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use crate::provider::prompt::Role;
use std::pin::Pin;

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicThinking {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: i64,
}

#[derive(Serialize)]
struct AnthropicPayload {
    model: String,
    messages: Vec<AnthropicMessage>,
    system: String,
    max_tokens: u32,
    stream: bool,
    // extended thinking rejects temperature/top_p/top_k outright, so these
    // are only sent when thinking is off; see `stream_completion` below.
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Deserialize)]
struct AnthropicEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<AnthropicDelta>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    text: Option<String>,
    thinking: Option<String>,
}

pub struct AnthropicProvider;

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn stream_completion(
        &self,
        http_client: reqwest::Client,
        base_url: String,
        api_key: String,
        model: String,
        messages: Vec<crate::provider::prompt::ChatMessage>,
        sampling: crate::provider::SamplingParams,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<String, ProviderError>> + Send>> {

        let mut anthropic_messages: Vec<AnthropicMessage> = Vec::new();
        let mut system_prompt = String::new();

        for msg in messages {
            if msg.role == Role::System {
                if system_prompt.is_empty() {
                    system_prompt = msg.content;
                } else {
                    system_prompt.push_str("\n");
                    system_prompt.push_str(&msg.content);
                }
            } else {
                // roles can only be 'user' or 'assistant'.
                // any 'system' roles that come later should just be 'user'.
                // but in our build_messages, system is only at the beginning.
                //
                // claude requires strict user/assistant alternation starting
                // with user and rejects a request with two consecutive
                // same-role turns outright - and build_messages can
                // legitimately produce those (e.g. a preset's own role="user"
                // prompt injections landing next to each other, or next to
                // the real user turn). merge into the previous turn instead
                // of trusting the assembled order to already alternate,
                // mirroring how the Gemini provider handles the same
                // requirement.
                let role = msg.role.as_str().to_string();
                if let Some(last) = anthropic_messages.last_mut() {
                    if last.role == role {
                        last.content.push_str("\n\n");
                        last.content.push_str(&msg.content);
                        continue;
                    }
                }
                anthropic_messages.push(AnthropicMessage { role, content: msg.content });
            }
        }

        // extended thinking won't take temperature/top_p/top_k in the same
        // request, and max_tokens has to leave room past the thinking budget
        let budget_tokens = sampling.reasoning_effort.to_budget_tokens();
        let base_max_tokens = if sampling.max_tokens > 0 { sampling.max_tokens as u32 } else { 8192 };
        let (max_tokens, temperature, top_p, top_k, thinking) = match budget_tokens {
            Some(budget) => (
                base_max_tokens.max(budget as u32 + 1024),
                None,
                None,
                None,
                Some(AnthropicThinking { thinking_type: "enabled".to_string(), budget_tokens: budget }),
            ),
            None => (
                base_max_tokens,
                Some(sampling.temperature),
                Some(sampling.top_p),
                if sampling.top_k > 0 { Some(sampling.top_k) } else { None },
                None,
            ),
        };

        let payload = AnthropicPayload {
            model,
            messages: anthropic_messages,
            system: system_prompt,
            max_tokens,
            stream: true,
            temperature,
            top_p,
            top_k,
            thinking,
        };

        Box::pin(async_stream::stream! {
            let mut headers = HeaderMap::new();
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
            let key_header = match HeaderValue::from_str(&api_key) {
                Ok(key) => key,
                Err(_) => {
                    // silently skipping this used to send an unauthenticated
                    // request instead - a confusing 401 from Anthropic rather
                    // than a clear local error about the key itself
                    yield Err(ProviderError::Status(400, "API key contains characters that can't be sent in an HTTP header".to_string()));
                    return;
                }
            };
            headers.insert("x-api-key", key_header);

            let url = if base_url.ends_with("/messages") {
                base_url.clone()
            } else if base_url.ends_with("/") {
                format!("{}v1/messages", base_url)
            } else {
                format!("{}/v1/messages", base_url)
            };

            let client = http_client;
            let response = client
                .post(url)
                .headers(headers)
                .json(&payload)
                .send()
                .await;

            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    yield Err(ProviderError::Request(e));
                    return;
                }
            };

            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                yield Err(ProviderError::Status(status.as_u16(), body));
                return;
            }

            let mut stream = response.bytes_stream();
            let mut pending_bytes: Vec<u8> = Vec::new();
            let mut buffer = String::new();
            // extended thinking streams its own content block ahead of the reply
            // (delta.thinking, not delta.text), same shape as the OpenAI-compatible
            // provider's separate `reasoning` field. synthesize `<think>` tags
            // around it so it lands in storage the same way regardless of provider.
            let mut in_reasoning = false;

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&super::decode_utf8_chunk(&mut pending_bytes, &bytes));

                        while let Some(idx) = buffer.find("\n\n") {
                            let event_block = buffer[..idx].to_string();
                            buffer = buffer[idx + 2..].to_string();

                            let mut event_type = "message".to_string();
                            let mut data = String::new();

                            for line in event_block.lines() {
                                if let Some(stripped) = line.strip_prefix("event: ") {
                                    event_type = stripped.trim().to_string();
                                } else if let Some(stripped) = line.strip_prefix("data: ") {
                                    data = stripped.trim().to_string();
                                }
                            }

                            if event_type == "content_block_delta" {
                                if let Ok(parsed) = serde_json::from_str::<AnthropicEvent>(&data) {
                                    if let Some(delta) = parsed.delta {
                                        if let Some(thinking) = delta.thinking {
                                            if !in_reasoning {
                                                yield Ok("<think>".to_string());
                                                in_reasoning = true;
                                            }
                                            yield Ok(thinking);
                                        }
                                        if let Some(text) = delta.text {
                                            if in_reasoning {
                                                yield Ok("</think>".to_string());
                                                in_reasoning = false;
                                            }
                                            yield Ok(text);
                                        }
                                    }
                                }
                            } else if event_type == "error" {
                                yield Ok(format!("\n[Anthropic Error: {}]", data));
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::Request(e));
                        return;
                    }
                }
            }

            if in_reasoning {
                yield Ok("</think>".to_string());
            }
        })
    }
}
