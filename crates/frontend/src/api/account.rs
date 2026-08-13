use gloo_net::http::Request;

pub async fn export_account() -> Result<String, String> {
    let response = Request::get("/api/account/export-all")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.ok() {
        return Err("failed to export account data".to_string());
    }
    response.text().await.map_err(|e| e.to_string())
}

pub async fn import_account(json_text: &str) -> Result<(), String> {
    let response = Request::post("/api/account/import-all")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .body(json_text)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(())
    } else {
        Err("failed to import account data - the file may not be a valid export".to_string())
    }
}

pub async fn delete_account_data(username: &str) -> Result<(), String> {
    let body = serde_json::json!({ "username": username }).to_string();
    let response = Request::delete("/api/account/data")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .body(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if response.ok() {
        Ok(())
    } else if response.status() == 403 {
        Err("username didn't match".to_string())
    } else {
        Err("failed to delete account data".to_string())
    }
}
