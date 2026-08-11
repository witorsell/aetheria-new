use serde::{Deserialize, Serialize};

/// everything update_settings writes in one call, deserialized straight
/// off the request body
#[derive(Deserialize)]
pub struct SettingsUpdate {
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub model_name: String,
    pub system_prompt: String,
    pub context_limit: i64,
    pub post_history_instructions: String,
    pub forbid_external_media: bool,
    pub provider_type: String,
    #[serde(default)]
    pub summary_provider_type: String,
    #[serde(default)]
    pub summary_api_base_url: String,
    #[serde(default)]
    pub summary_api_key: Option<String>,
    #[serde(default)]
    pub summary_model_name: String,
    #[serde(default)]
    pub summary_context_limit: Option<i64>,
    #[serde(default)]
    pub embedding_source: String,
    #[serde(default)]
    pub embedding_api_base_url: String,
    #[serde(default)]
    pub embedding_api_key: Option<String>,
    #[serde(default)]
    pub embedding_model_name: String,
    #[serde(default = "default_rag_top_k")]
    pub rag_top_k: i64,
    #[serde(default = "default_rag_score_threshold")]
    pub rag_score_threshold: f64,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default)]
    pub top_k: i64,
    #[serde(default)]
    pub frequency_penalty: f64,
    #[serde(default)]
    pub presence_penalty: f64,
    #[serde(default)]
    pub max_response_tokens: i64,
    // "" = don't send it, same disabled-sentinel as the other sampling fields
    #[serde(default)]
    pub reasoning_effort: String,
}

// backup/transfer snapshot: everything in SettingsUpdate minus API keys
// and identity stuff (that lives on the user record, not here)
#[derive(Serialize, Deserialize)]
pub struct SettingsExport {
    pub api_base_url: String,
    pub model_name: String,
    pub system_prompt: String,
    pub context_limit: i64,
    pub post_history_instructions: String,
    pub forbid_external_media: bool,
    pub provider_type: String,
    pub active_preset_id: Option<String>,
    pub summary_provider_type: String,
    pub summary_api_base_url: String,
    pub summary_model_name: String,
    pub summary_context_limit: Option<i64>,
    pub embedding_source: String,
    pub embedding_api_base_url: String,
    pub embedding_model_name: String,
    pub rag_top_k: i64,
    pub rag_score_threshold: f64,
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub frequency_penalty: f64,
    pub presence_penalty: f64,
    pub max_response_tokens: i64,
    pub reasoning_effort: String,
}

fn default_rag_top_k() -> i64 {
    5
}
fn default_rag_score_threshold() -> f64 {
    0.5
}
fn default_temperature() -> f64 {
    1.0
}
fn default_top_p() -> f64 {
    1.0
}

#[derive(Serialize)]
pub struct SettingsView {
    pub user_id: i64,
    pub api_base_url: String,
    pub model_name: String,
    pub system_prompt: String,
    pub has_api_key: bool,
    pub context_limit: i64,
    pub post_history_instructions: String,
    pub forbid_external_media: bool,
    pub provider_type: String,
    pub active_preset_id: Option<String>,
    // empty/unset fields fall back to the main provider settings, see
    // get_summary_config. summary_context_limit None = inherit main limit
    pub summary_provider_type: String,
    pub summary_api_base_url: String,
    pub has_summary_api_key: bool,
    pub summary_model_name: String,
    pub summary_context_limit: Option<i64>,
    // "" = off, "local" = free on-server model, "api" = OpenAI-compatible
    // /embeddings endpoint (model name has no fallback, unlike base_url/key)
    pub embedding_source: String,
    pub embedding_api_base_url: String,
    pub has_embedding_api_key: bool,
    pub embedding_model_name: String,
    pub rag_top_k: i64,
    pub rag_score_threshold: f64,
    // top_k 0 = don't send it, same for frequency/presence_penalty/max_response_tokens
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub frequency_penalty: f64,
    pub presence_penalty: f64,
    pub max_response_tokens: i64,
    pub reasoning_effort: String,
}

impl SettingsView {
    pub fn sampling_params(&self) -> crate::provider::SamplingParams {
        crate::provider::SamplingParams {
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            max_tokens: self.max_response_tokens,
            reasoning_effort: self.reasoning_effort.clone(),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SettingsRow {
    user_id: i64,
    api_base_url: String,
    api_key: String,
    model_name: String,
    system_prompt: String,
    context_limit: i64,
    post_history_instructions: String,
    forbid_external_media: bool,
    provider_type: String,
    active_preset_id: Option<String>,
    summary_provider_type: String,
    summary_api_base_url: String,
    summary_api_key: String,
    summary_model_name: String,
    summary_context_limit: Option<i64>,
    embedding_source: String,
    embedding_api_base_url: String,
    embedding_api_key: String,
    embedding_model_name: String,
    rag_top_k: i64,
    rag_score_threshold: f64,
    temperature: f64,
    top_p: f64,
    top_k: i64,
    frequency_penalty: f64,
    presence_penalty: f64,
    max_response_tokens: i64,
    reasoning_effort: String,
}

pub async fn get_view(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<SettingsView> {
    let row = sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(SettingsView { user_id,
        api_base_url: row.api_base_url,
        model_name: row.model_name,
        system_prompt: row.system_prompt,
        has_api_key: !row.api_key.is_empty(),
        context_limit: row.context_limit,
        post_history_instructions: row.post_history_instructions,
        forbid_external_media: row.forbid_external_media,
        provider_type: row.provider_type,
        active_preset_id: row.active_preset_id,
        summary_provider_type: row.summary_provider_type,
        summary_api_base_url: row.summary_api_base_url,
        has_summary_api_key: !row.summary_api_key.is_empty(),
        summary_model_name: row.summary_model_name,
        summary_context_limit: row.summary_context_limit,
        embedding_source: row.embedding_source,
        embedding_api_base_url: row.embedding_api_base_url,
        has_embedding_api_key: !row.embedding_api_key.is_empty(),
        embedding_model_name: row.embedding_model_name,
        rag_top_k: row.rag_top_k,
        rag_score_threshold: row.rag_score_threshold,
        temperature: row.temperature,
        top_p: row.top_p,
        top_k: row.top_k,
        frequency_penalty: row.frequency_penalty,
        presence_penalty: row.presence_penalty,
        max_response_tokens: row.max_response_tokens,
        reasoning_effort: row.reasoning_effort,
    })
}

pub async fn get_export(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<SettingsExport> {
    let row = sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(SettingsExport {
        api_base_url: row.api_base_url,
        model_name: row.model_name,
        system_prompt: row.system_prompt,
        context_limit: row.context_limit,
        post_history_instructions: row.post_history_instructions,
        forbid_external_media: row.forbid_external_media,
        provider_type: row.provider_type,
        active_preset_id: row.active_preset_id,
        summary_provider_type: row.summary_provider_type,
        summary_api_base_url: row.summary_api_base_url,
        summary_model_name: row.summary_model_name,
        summary_context_limit: row.summary_context_limit,
        embedding_source: row.embedding_source,
        embedding_api_base_url: row.embedding_api_base_url,
        embedding_model_name: row.embedding_model_name,
        rag_top_k: row.rag_top_k,
        rag_score_threshold: row.rag_score_threshold,
        temperature: row.temperature,
        top_p: row.top_p,
        top_k: row.top_k,
        frequency_penalty: row.frequency_penalty,
        presence_penalty: row.presence_penalty,
        max_response_tokens: row.max_response_tokens,
        reasoning_effort: row.reasoning_effort,
    })
}

impl SettingsExport {
    // None for every key so update_settings leaves the stored ones alone
    pub fn into_update(self) -> SettingsUpdate {
        SettingsUpdate {
            api_base_url: self.api_base_url,
            api_key: None,
            model_name: self.model_name,
            system_prompt: self.system_prompt,
            context_limit: self.context_limit,
            post_history_instructions: self.post_history_instructions,
            forbid_external_media: self.forbid_external_media,
            provider_type: self.provider_type,
            summary_provider_type: self.summary_provider_type,
            summary_api_base_url: self.summary_api_base_url,
            summary_api_key: None,
            summary_model_name: self.summary_model_name,
            summary_context_limit: self.summary_context_limit,
            embedding_source: self.embedding_source,
            embedding_api_base_url: self.embedding_api_base_url,
            embedding_api_key: None,
            embedding_model_name: self.embedding_model_name,
            rag_top_k: self.rag_top_k,
            rag_score_threshold: self.rag_score_threshold,
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            frequency_penalty: self.frequency_penalty,
            presence_penalty: self.presence_penalty,
            max_response_tokens: self.max_response_tokens,
            reasoning_effort: self.reasoning_effort,
        }
    }
}

// single shared fetch of the settings row so a single generation pass doesn't run 5 separate SELECTs
async fn fetch_settings_row(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<SettingsRow> {
    sqlx::query_as::<_, SettingsRow>("SELECT * FROM settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

// used by the provider client, not exposed over the API.
pub async fn get_decrypted_api_key(pool: &sqlx::SqlitePool, user_id: i64, encryption_key: &[u8; 32]) -> sqlx::Result<String> {
    let row = fetch_settings_row(pool, user_id).await?;
    if row.api_key.is_empty() {
        return Ok(String::new());
    }
    crate::crypto::decrypt(encryption_key, &row.api_key)
        .map_err(|e| sqlx::Error::Configuration(format!("API key decryption failed: {e}").into()))
}

// resolved provider/model/context for background summarization: summary_*
// overrides where set, main provider settings for whatever's left blank
pub struct SummaryConfig {
    pub provider_type: String,
    pub api_base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub context_limit: i64,
}

pub async fn get_summary_config(pool: &sqlx::SqlitePool, user_id: i64, encryption_key: &[u8; 32]) -> sqlx::Result<SummaryConfig> {
    let row = fetch_settings_row(pool, user_id).await?;

    let provider_type = if row.summary_provider_type.is_empty() { row.provider_type } else { row.summary_provider_type };
    let api_base_url = if row.summary_api_base_url.is_empty() { row.api_base_url } else { row.summary_api_base_url };
    let api_key_encrypted = if row.summary_api_key.is_empty() { &row.api_key } else { &row.summary_api_key };
    let api_key = if api_key_encrypted.is_empty() {
        String::new()
    } else {
        crate::crypto::decrypt(encryption_key, api_key_encrypted)
            .map_err(|e| sqlx::Error::Configuration(format!("Summary API key decryption failed: {e}").into()))?
    };
    let model_name = if row.summary_model_name.is_empty() { row.model_name } else { row.summary_model_name };
    let context_limit = row.summary_context_limit.unwrap_or(row.context_limit);

    Ok(SummaryConfig { provider_type, api_base_url, api_key, model_name, context_limit })
}

pub struct RagParams {
    pub top_k: usize,
    pub score_threshold: f32,
}

pub async fn get_rag_params(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<RagParams> {
    let row = fetch_settings_row(pool, user_id).await?;
    Ok(RagParams { top_k: row.rag_top_k.max(0) as usize, score_threshold: row.rag_score_threshold as f32 })
}

// None = feature's off
pub enum EmbeddingBackend {
    Local,
    // model name has no fallback, unlike base_url/key
    Api { api_base_url: String, api_key: String, model_name: String },
}

pub async fn get_embedding_config(pool: &sqlx::SqlitePool, user_id: i64, encryption_key: &[u8; 32]) -> sqlx::Result<Option<EmbeddingBackend>> {
    let row = fetch_settings_row(pool, user_id).await?;

    match row.embedding_source.as_str() {
        "local" => Ok(Some(EmbeddingBackend::Local)),
        "api" if !row.embedding_model_name.is_empty() => {
            let api_base_url = if row.embedding_api_base_url.is_empty() { row.api_base_url } else { row.embedding_api_base_url };
            let api_key_encrypted = if row.embedding_api_key.is_empty() { &row.api_key } else { &row.embedding_api_key };
            let api_key = if api_key_encrypted.is_empty() {
                String::new()
            } else {
                crate::crypto::decrypt(encryption_key, api_key_encrypted)
                    .map_err(|e| sqlx::Error::Configuration(format!("Embedding API key decryption failed: {e}").into()))?
            };
            Ok(Some(EmbeddingBackend::Api { api_base_url, api_key, model_name: row.embedding_model_name }))
        }
        _ => Ok(None),
    }
}
