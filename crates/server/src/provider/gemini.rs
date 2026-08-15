use super::{ModelProvider, ProviderError};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use crate::provider::prompt::Role;
use std::pin::Pin;

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiSafetySetting {
    category: String,
    threshold: String,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiThinkingConfig {
    thinking_budget: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    top_k: i64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    frequency_penalty: f64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    presence_penalty: f64,
    // unlike Anthropic (max_tokens is a required field) or NovelAI/Horde
    // (max_length isn't optional in their payload schema either), Gemini's
    // maxOutputTokens is genuinely optional - omitting it lets the model use
    // its own default rather than the caller's. so 0 (= "disabled" per
    // SamplingParams's documented contract) can actually map to "omit" here
    // instead of substituting an arbitrary fallback value
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_config: Option<GeminiThinkingConfig>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPayload {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    safety_settings: Vec<GeminiSafetySetting>,
    generation_config: GeminiGenerationConfig,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResponseContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiResponseContent>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

pub struct GeminiProvider;

#[async_trait]
impl ModelProvider for GeminiProvider {
    async fn stream_completion(
        &self,
        http_client: reqwest::Client,
        base_url: String,
        api_key: String,
        model: String,
        messages: Vec<crate::provider::prompt::ChatMessage>,
        sampling: crate::provider::SamplingParams,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<String, ProviderError>> + Send>> {
        let mut contents: Vec<GeminiContent> = Vec::new();
        let mut system_instruction = None;

        for msg in messages {
            if msg.role == Role::System {
                if system_instruction.is_none() {
                    system_instruction = Some(GeminiContent {
                        role: "user".to_string(), // system_instruction technically doesn't need role, but we provide it
                        parts: vec![GeminiPart { text: msg.content }],
                    });
                } else if let Some(ref mut sys) = system_instruction {
                    if let Some(part) = sys.parts.first_mut() {
                        part.text.push_str("\n\n");
                        part.text.push_str(&msg.content);
                    }
                }
            } else {
                let role = if msg.role == Role::User { "user" } else { "model" };
                
                if let Some(last) = contents.last_mut() {
                    if last.role == role {
                        if let Some(part) = last.parts.first_mut() {
                            part.text.push_str("\n\n");
                            part.text.push_str(&msg.content);
                        }
                        continue;
                    }
                }

                contents.push(GeminiContent {
                    role: role.to_string(),
                    parts: vec![GeminiPart { text: msg.content }],
                });
            }
        }

        // Gemini requires safety settings to bypass blocks
        let safety_settings = vec![
            GeminiSafetySetting { category: "HARM_CATEGORY_HARASSMENT".to_string(), threshold: "BLOCK_NONE".to_string() },
            GeminiSafetySetting { category: "HARM_CATEGORY_HATE_SPEECH".to_string(), threshold: "BLOCK_NONE".to_string() },
            GeminiSafetySetting { category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(), threshold: "BLOCK_NONE".to_string() },
            GeminiSafetySetting { category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(), threshold: "BLOCK_NONE".to_string() },
            GeminiSafetySetting { category: "HARM_CATEGORY_CIVIC_INTEGRITY".to_string(), threshold: "BLOCK_NONE".to_string() },
        ];

        let payload = GeminiPayload {
            contents,
            system_instruction,
            safety_settings,
            generation_config: GeminiGenerationConfig {
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: sampling.top_k,
                frequency_penalty: sampling.frequency_penalty,
                presence_penalty: sampling.presence_penalty,
                max_output_tokens: if sampling.max_tokens > 0 { Some(sampling.max_tokens as i32) } else { None },
                thinking_config: sampling
                    .reasoning_effort
                    .to_budget_tokens()
                    .map(|thinking_budget| GeminiThinkingConfig { thinking_budget }),
            },
        };

        Box::pin(async_stream::stream! {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));
            // sent as a header rather than the ?key= query param Google's
            // docs also accept - a query param ends up verbatim in server
            // access logs and any proxy in front of this request, unlike
            // Anthropic/OpenAI/NovelAI which already authenticate via header
            let key_header = match HeaderValue::from_str(&api_key) {
                Ok(key) => key,
                Err(_) => {
                    yield Err(ProviderError::Status(400, "API key contains characters that can't be sent in an HTTP header".to_string()));
                    return;
                }
            };
            headers.insert("x-goog-api-key", key_header);

            let base = if base_url.is_empty() {
                "https://generativelanguage.googleapis.com".to_string()
            } else {
                base_url.trim_end_matches('/').to_string()
            };

            let url = format!("{}/v1beta/models/{}:streamGenerateContent?alt=sse", base, model);

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

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&super::decode_utf8_chunk(&mut pending_bytes, &bytes));

                        while let Some(idx) = buffer.find('\n') {
                            let line = buffer[..idx].trim().to_string();
                            buffer = buffer[idx + 1..].to_string();
                            
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data == "[DONE]" {
                                    continue;
                                }
                                if let Ok(parsed) = serde_json::from_str::<GeminiResponse>(data) {
                                    if let Some(candidates) = parsed.candidates {
                                        if let Some(candidate) = candidates.first() {
                                            if let Some(content) = &candidate.content {
                                                if let Some(parts) = &content.parts {
                                                    if let Some(part) = parts.first() {
                                                        if let Some(text) = &part.text {
                                                            yield Ok(text.clone());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(ProviderError::Request(e));
                        return;
                    }
                }
            }
        })
    }
}
