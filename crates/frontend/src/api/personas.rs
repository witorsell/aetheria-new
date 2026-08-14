use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, PartialEq)]
pub struct Persona {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar_url: Option<String>,
}

#[derive(Serialize)]
pub struct PersonaInput<'a> {
    pub name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
}

pub async fn list_personas() -> Result<Vec<Persona>, String> {
    Request::get("/api/personas")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_persona(input: PersonaInput<'_>) -> Result<Persona, String> {
    Request::post("/api/personas")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_persona(id: &str, input: PersonaInput<'_>) -> Result<(), String> {
    // gloo-net 0.7 has a Request::patch shorthand same as get/post/put/delete, no need for the new().method() dance
    let resp = Request::patch(&format!("/api/personas/{id}"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err("failed to update persona".to_string())
    }
}

pub async fn delete_persona(id: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/personas/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    Ok(())
}

pub async fn set_active_persona(persona_id: Option<String>) -> Result<(), String> {
    let resp = Request::post("/api/personas/active")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&serde_json::json!({ "persona_id": persona_id }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.ok() {
        Ok(())
    } else {
        Err("failed to set active persona".to_string())
    }
}

// mirrors the inline avatar upload handler in character_editor.rs, raw fetch + js_sys::Reflect since serde_wasm_bindgen isn't a dependency here
pub async fn upload_persona_avatar(persona_id: &str, file: web_sys::File) -> Result<String, String> {
    use wasm_bindgen::JsCast;

    let form_data = web_sys::FormData::new().map_err(|_| "Failed to create FormData".to_string())?;
    form_data.append_with_blob("avatar", &file).map_err(|_| "Failed to append file".to_string())?;

    let url = format!("/api/personas/{persona_id}/avatar");
    let req_init = web_sys::RequestInit::new();
    req_init.set_method("POST");
    req_init.set_body(&form_data);
    let request = web_sys::Request::new_with_str_and_init(&url, &req_init).map_err(|_| "Failed to build request".to_string())?;
    let window = web_sys::window().ok_or("No window")?;
    let resp_val = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "Upload failed".to_string())?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| "Bad response".to_string())?;
    if !resp.ok() {
        return Err("Upload failed".to_string());
    }
    let json_val = wasm_bindgen_futures::JsFuture::from(resp.json().map_err(|_| "Bad response body".to_string())?)
        .await
        .map_err(|_| "Bad response body".to_string())?;
    js_sys::Reflect::get(&json_val, &wasm_bindgen::JsValue::from_str("avatar_url"))
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| "missing avatar_url in response".to_string())
}
