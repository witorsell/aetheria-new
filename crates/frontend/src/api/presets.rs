use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct PresetPrompt {
    pub identifier: String,
    pub name: String,
    pub content: String,
    pub role: String,
    pub marker: bool,
    pub injection_position: i32,
    pub injection_depth: i32,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct PresetOrderEntry {
    pub identifier: String,
    pub enabled: bool,
}

#[derive(Clone, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub prompts: Vec<PresetPrompt>,
    pub prompt_order: Vec<PresetOrderEntry>,
}

#[derive(Serialize)]
struct UpdatePresetOrderInput {
    prompt_order: Vec<PresetOrderEntry>,
}

#[derive(Serialize)]
struct ActivatePresetInput<'a> {
    preset_id: Option<&'a str>,
}

pub async fn list_presets() -> Result<Vec<Preset>, String> {
    Request::get("/api/presets")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_preset(id: &str) -> Result<Preset, String> {
    Request::get(&format!("/api/presets/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn export_preset(id: &str) -> Result<String, String> {
    Request::get(&format!("/api/presets/{}/export", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_preset_order(id: &str, prompt_order: Vec<PresetOrderEntry>) -> Result<(), String> {
    Request::put(&format!("/api/presets/{}/order", id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&UpdatePresetOrderInput { prompt_order })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn import_preset(file: web_sys::File) -> Result<Preset, String> {
    let form = web_sys::FormData::new().map_err(|_| "Failed to create FormData")?;
    form.append_with_blob("file", &file).map_err(|_| "Failed to append file")?;

    let resp = Request::post("/api/presets")
        .credentials(web_sys::RequestCredentials::Include)
        .body(form)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn delete_preset(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/presets/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn activate_preset(preset_id: Option<&str>) -> Result<(), String> {
    Request::post("/api/presets/activate")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&ActivatePresetInput { preset_id })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
