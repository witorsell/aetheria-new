use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize)]
pub struct RegexScript {
    pub id: String,
    pub script_name: String,
    pub find_regex: String,
    pub replace_string: String,
    pub placement: Vec<i32>,
    pub disabled: bool,
    pub markdown_only: bool,
    pub prompt_only: bool,
    pub min_depth: Option<i32>,
    pub max_depth: Option<i32>,
}

#[derive(Serialize)]
struct SetDisabledInput {
    disabled: bool,
}

pub async fn list_regex_scripts() -> Result<Vec<RegexScript>, String> {
    Request::get("/api/regex-scripts")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn export_regex_scripts() -> Result<String, String> {
    Request::get("/api/regex-scripts/export")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}

pub async fn import_regex_scripts(file: web_sys::File) -> Result<Vec<RegexScript>, String> {
    let form = web_sys::FormData::new().map_err(|_| "Failed to create FormData")?;
    form.append_with_blob("file", &file).map_err(|_| "Failed to append file")?;

    let resp = Request::post("/api/regex-scripts")
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

pub async fn delete_regex_script(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/regex-scripts/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn set_regex_script_disabled(id: &str, disabled: bool) -> Result<(), String> {
    Request::put(&format!("/api/regex-scripts/{}/disabled", id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&SetDisabledInput { disabled })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}
