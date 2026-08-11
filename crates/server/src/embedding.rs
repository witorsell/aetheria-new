use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum EmbedMode {
    Query,
    Document,
}

#[cfg(feature = "local-embeddings")]
impl From<EmbedMode> for crate::local_embedding::EmbedMode {
    fn from(m: EmbedMode) -> Self {
        match m {
            EmbedMode::Query => crate::local_embedding::EmbedMode::Query,
            EmbedMode::Document => crate::local_embedding::EmbedMode::Document,
        }
    }
}

/// embeds a batch of texts using whichever backend is configured, local or
/// API. this is the entry point `vector_memory` calls; it doesn't need to
/// know which one it's talking to. `mode` only matters for the local
/// backend (nomic-embed-text embeds queries and stored documents
/// asymmetrically); the API path ignores it.
pub async fn embed(
    client: &reqwest::Client,
    backend: &crate::models::settings::EmbeddingBackend,
    inputs: &[String],
    mode: EmbedMode,
) -> Result<Vec<Vec<f32>>, String> {
    match backend {
        #[cfg(feature = "local-embeddings")]
        crate::models::settings::EmbeddingBackend::Local => crate::local_embedding::embed(inputs, mode.into()).await,
        #[cfg(not(feature = "local-embeddings"))]
        crate::models::settings::EmbeddingBackend::Local => {
            Err("Local embeddings feature ('local-embeddings') is not enabled in this server build.".to_string())
        }
        crate::models::settings::EmbeddingBackend::Api { api_base_url, api_key, model_name } => {
            embed_api(client, api_base_url, api_key, model_name, inputs).await
        }
    }
}

/// calls an openai-compatible /embeddings endpoint for a batch in one
/// request. re-sorts by the response's own index field, don't trust
/// request order to survive every provider
pub async fn embed_api(
    client: &reqwest::Client,
    api_base_url: &str,
    api_key: &str,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let response = client
        .post(format!("{}/embeddings", api_base_url.trim_end_matches('/')))
        .bearer_auth(api_key)
        .json(&EmbeddingsRequest { model, input: inputs })
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("embeddings request failed ({status}): {body}"));
    }

    let mut parsed: EmbeddingsResponse = response.json().await.map_err(|e| e.to_string())?;
    parsed.data.sort_by_key(|item| item.index);
    Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
}

pub fn pack(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn unpack(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_round_trips() {
        let original = vec![0.5f32, -1.25, 3.0, 0.0, -0.001];
        let packed = pack(&original);
        let unpacked = unpack(&packed);
        assert_eq!(original, unpacked);
    }

    #[test]
    fn identical_vectors_have_similarity_one() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors_have_similarity_zero() {
        let a = vec![1.0f32, 0.0];
        let b = vec![0.0f32, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors_have_similarity_negative_one() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }
}
