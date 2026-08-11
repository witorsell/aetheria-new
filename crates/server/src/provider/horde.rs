use super::{ModelProvider, ProviderError};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use crate::provider::prompt::Role;

#[derive(Serialize)]
struct HordePayload {
    prompt: String,
    params: HordeParams,
    models: Vec<String>,
    workers: Vec<String>,
    trusted_workers: bool,
}

#[derive(Serialize)]
struct HordeParams {
    n: i32,
    max_length: i32,
    temperature: f64,
    top_p: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<i64>,
}

#[derive(Deserialize)]
struct HordeAsyncResponse {
    id: String,
    message: Option<String>,
}

#[derive(Deserialize)]
struct HordeStatusGeneration {
    text: String,
}

#[derive(Deserialize)]
struct HordeStatusResponse {
    generations: Option<Vec<HordeStatusGeneration>>,
    done: bool,
    faulted: bool,
    message: Option<String>,
}

pub struct HordeProvider;

#[async_trait]
impl ModelProvider for HordeProvider {
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
        
        let payload = HordePayload {
            prompt: prompt_string,
            params: HordeParams {
                n: 1,
                max_length: if sampling.max_tokens > 0 { sampling.max_tokens as i32 } else { 512 },
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: if sampling.top_k > 0 { Some(sampling.top_k) } else { None },
            },
            models: if model.is_empty() || model == "any" { vec![] } else { vec![model] },
            workers: vec![],
            trusted_workers: false,
        };

        Box::pin(async_stream::stream! {
            let mut headers = HeaderMap::new();
            if let Ok(key) = HeaderValue::from_str(&api_key) {
                headers.insert("apikey", key);
            } else {
                headers.insert("apikey", HeaderValue::from_static("0000000000"));
            }
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));

            let base = if base_url.is_empty() {
                "https://aihorde.net/api/v2".to_string()
            } else {
                base_url.trim_end_matches('/').to_string()
            };

            let url = format!("{}/generate/text/async", base);
            
            let client = reqwest::Client::new();
            let response = client
                .post(url)
                .headers(headers.clone())
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

            let init: HordeAsyncResponse = match response.json().await {
                Ok(r) => r,
                Err(e) => {
                    yield Err(ProviderError::Request(e));
                    return;
                }
            };

            let status_url = format!("{}/generate/text/status/{}", base, init.id);
            
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;

                let res = client.get(&status_url).headers(headers.clone()).send().await;
                let res = match res {
                    Ok(r) => r,
                    Err(e) => {
                        yield Err(ProviderError::Request(e));
                        return;
                    }
                };

                let status = res.status();
                if !status.is_success() {
                    let body = res.text().await.unwrap_or_default();
                    yield Err(ProviderError::Status(status.as_u16(), body));
                    return;
                }

                let check: HordeStatusResponse = match res.json().await {
                    Ok(r) => r,
                    Err(e) => {
                        yield Err(ProviderError::Request(e));
                        return;
                    }
                };

                if check.faulted {
                    yield Err(ProviderError::Status(500, "Worker faulted".to_string()));
                    return;
                }

                if check.done {
                    if let Some(gens) = check.generations {
                        if let Some(first) = gens.first() {
                            yield Ok(first.text.clone());
                        }
                    }
                    return;
                }
            }
        })
    }
}
