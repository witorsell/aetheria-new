use super::{ModelProvider, ProviderError};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use crate::provider::prompt::Role;
use std::pin::Pin;

#[derive(Serialize)]
struct NovelPayload {
    input: String,
    model: String,
    parameters: NovelParameters,
}

#[derive(Serialize)]
struct NovelParameters {
    max_length: i32,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i64>,
    repetition_penalty: f32,
}

#[derive(Deserialize)]
struct NovelResponse {
    token: Option<String>,
}

pub struct NovelProvider;

#[async_trait]
impl ModelProvider for NovelProvider {
    async fn stream_completion(
        &self,
        base_url: String,
        api_key: String,
        model: String,
        messages: Vec<crate::provider::prompt::ChatMessage>,
        sampling: crate::provider::SamplingParams,
    ) -> Pin<Box<dyn futures_util::Stream<Item = Result<String, ProviderError>> + Send>> {
        let mut prompt_string = String::new();
        
        for msg in messages {
            if msg.role == Role::System {
                prompt_string.push_str(&msg.content);
                prompt_string.push_str("\n***\n");
            } else if msg.role == Role::User {
                prompt_string.push_str(&msg.content);
                prompt_string.push_str("\n");
            } else if msg.role == Role::Assistant {
                prompt_string.push_str(&msg.content);
                prompt_string.push_str("\n");
            }
        }
        
        let payload = NovelPayload {
            input: prompt_string,
            model,
            parameters: NovelParameters {
                max_length: if sampling.max_tokens > 0 { sampling.max_tokens as i32 } else { 500 },
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: if sampling.top_k > 0 { Some(sampling.top_k) } else { None },
                repetition_penalty: 1.0,
            },
        };

        Box::pin(async_stream::stream! {
            let mut headers = HeaderMap::new();
            if let Ok(key) = HeaderValue::from_str(&format!("Bearer {}", api_key)) {
                headers.insert("Authorization", key);
            }
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));

            let url = format!("{}/ai/generate-stream", base_url.trim_end_matches('/'));
            
            let client = reqwest::Client::new();
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
                            
                            if let Some(data) = line.strip_prefix("data:") {
                                if let Ok(parsed) = serde_json::from_str::<NovelResponse>(data) {
                                    if let Some(token) = parsed.token {
                                        yield Ok(token);
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
