use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ModelListItem {
    pub id: String,
}

#[derive(Deserialize, Clone)]
pub struct SettingsView {
    pub api_base_url: String,
    pub model_name: String,
    pub system_prompt: String,
    pub has_api_key: bool,
    pub context_limit: i64,
    pub post_history_instructions: String,
    pub forbid_external_media: bool,
    pub provider_type: String,
    pub active_preset_id: Option<String>,
    pub summary_provider_type: String,
    pub summary_api_base_url: String,
    pub has_summary_api_key: bool,
    pub summary_model_name: String,
    pub summary_context_limit: Option<i64>,
    pub embedding_source: String,
    pub embedding_api_base_url: String,
    pub has_embedding_api_key: bool,
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

#[derive(Clone, Deserialize, Serialize)]
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

#[derive(Clone, Default, Serialize)]
pub struct UpdateSettingsRequest {
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub model_name: String,
    pub system_prompt: String,
    pub context_limit: i64,
    pub post_history_instructions: String,
    pub forbid_external_media: bool,
    pub provider_type: String,
    pub summary_provider_type: String,
    pub summary_api_base_url: String,
    pub summary_api_key: Option<String>,
    pub summary_model_name: String,
    pub summary_context_limit: Option<i64>,
    pub embedding_source: String,
    pub embedding_api_base_url: String,
    pub embedding_api_key: Option<String>,
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

#[derive(Clone, Deserialize, Serialize)]
pub struct MeResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub active_persona_id: Option<String>,
    pub persona_name: Option<String>,
    pub persona_avatar_url: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct UpdateMeRequest {
    pub display_name: Option<String>,
}

pub async fn get_settings() -> Result<SettingsView, String> {
    Request::get("/api/settings")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_settings(req: UpdateSettingsRequest) -> Result<(), String> {
    let response = Request::put("/api/settings")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(())
    } else {
        Err("failed to save settings".to_string())
    }
}

pub async fn export_settings() -> Result<SettingsExport, String> {
    Request::get("/api/settings/export")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn import_settings(export: &SettingsExport) -> Result<(), String> {
    let response = Request::post("/api/settings/import")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(export)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(())
    } else {
        Err("failed to import settings".to_string())
    }
}

pub async fn list_models(subscription_only: bool) -> Result<Vec<ModelListItem>, String> {
    let url = if subscription_only {
        "/api/settings/models?subscription_only=true"
    } else {
        "/api/settings/models"
    };
    let response = Request::get(url)
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.ok() {
        response.json().await.map_err(|e| e.to_string())
    } else {
        let body = response.text().await.unwrap_or_default();
        if body.is_empty() {
            Err("could not fetch the model list, check the API base URL and key are saved and correct".to_string())
        } else {
            Err(body)
        }
    }
}

pub async fn fetch_me() -> Result<MeResponse, String> {
    Request::get("/api/me")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_me(req: UpdateMeRequest) -> Result<(), String> {
    Request::put("/api/me")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
