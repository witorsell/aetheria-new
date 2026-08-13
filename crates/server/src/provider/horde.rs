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

// AI Horde is a volunteer-worker queue, not a real-time API - a submitted
// job can sit unpicked for a long time under load, but an abandoned/orphaned
// one should never poll forever. bounds how long stream_completion will
// keep waiting on one job before giving up.
const MAX_POLL_DURATION: std::time::Duration = std::time::Duration::from_secs(600);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(2500);

pub struct HordeProvider;

#[async_trait]
impl ModelProvider for HordeProvider {
    async fn stream_completion(
        &self,
        http_client: reqwest::Client,
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
            // "0000000000" is Horde's own well-known anonymous key, the
            // correct fallback when no key was configured at all. but a
            // non-empty key that just fails to parse as a header value is a
            // real typo/corruption, not a deliberate choice to go anonymous -
            // silently reinterpreting it as anonymous used to hide that.
            if api_key.trim().is_empty() {
                headers.insert("apikey", HeaderValue::from_static("0000000000"));
            } else {
                match HeaderValue::from_str(&api_key) {
                    Ok(key) => { headers.insert("apikey", key); }
                    Err(_) => {
                        yield Err(ProviderError::Status(400, "API key contains characters that can't be sent in an HTTP header".to_string()));
                        return;
                    }
                }
            }
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));

            let base = if base_url.is_empty() {
                "https://aihorde.net/api/v2".to_string()
            } else {
                base_url.trim_end_matches('/').to_string()
            };

            let url = format!("{}/generate/text/async", base);

            let client = http_client;
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
            let poll_start = tokio::time::Instant::now();

            loop {
                if poll_start.elapsed() > MAX_POLL_DURATION {
                    yield Err(ProviderError::Status(504, "Horde generation timed out waiting for a worker to pick up the job".to_string()));
                    return;
                }

                tokio::time::sleep(POLL_INTERVAL).await;

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
                    match check.generations.as_ref().and_then(|gens| gens.first()) {
                        Some(first) => yield Ok(first.text.clone()),
                        None => yield Err(ProviderError::Status(502, "Horde marked the job done but returned no generation".to_string())),
                    }
                    return;
                }
            }
        })
    }
}
