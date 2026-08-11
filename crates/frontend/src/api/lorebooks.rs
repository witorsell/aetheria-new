use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Lorebook {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scan_depth: i64,
    pub token_budget: i64,
    pub recursive_scanning: bool,
    pub extensions: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct LorebookEntry {
    pub id: String,
    pub lorebook_id: String,
    pub name: String,
    pub entry: String,
    pub keywords: String,
    pub priority: i64,
    pub weight: i64,
    pub enabled: bool,
    pub comment: String,
    pub secondary_keys: String,
    pub constant: bool,
    pub position: String,
    pub probability: i64,
    pub use_probability: bool,
    pub selective: bool,
    pub selective_logic: i64,
    pub exclude_recursion: bool,
}

#[derive(Clone, Serialize)]
pub struct CreateLorebookInput {
    pub name: String,
    pub description: Option<String>,
    pub scan_depth: Option<i64>,
    pub token_budget: Option<i64>,
    pub recursive_scanning: Option<bool>,
    pub extensions: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct CreateLorebookEntryInput {
    pub lorebook_id: String,
    pub name: String,
    pub entry: String,
    pub keywords: Option<String>,
    pub priority: Option<i64>,
    pub weight: Option<i64>,
    pub enabled: Option<bool>,
    pub comment: Option<String>,
    pub secondary_keys: Option<String>,
    pub constant: Option<bool>,
    pub position: Option<String>,
    pub probability: Option<i64>,
    pub use_probability: Option<bool>,
    pub selective: Option<bool>,
    pub selective_logic: Option<i64>,
    pub exclude_recursion: Option<bool>,
}

#[derive(Serialize)]
pub struct SetLorebooksRequest {
    pub lorebook_ids: Vec<String>,
}

pub async fn list_lorebooks() -> Result<Vec<Lorebook>, String> {
    Request::get("/api/lorebooks")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_lorebook(id: &str) -> Result<Lorebook, String> {
    Request::get(&format!("/api/lorebooks/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_lorebook(input: &CreateLorebookInput) -> Result<Lorebook, String> {
    Request::post("/api/lorebooks")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_lorebook(id: &str, input: &CreateLorebookInput) -> Result<(), String> {
    Request::put(&format!("/api/lorebooks/{}", id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn delete_lorebook(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/lorebooks/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn list_lorebook_entries(lorebook_id: &str) -> Result<Vec<LorebookEntry>, String> {
    Request::get(&format!("/api/lorebooks/{}/entries", lorebook_id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_lorebook_entry(lorebook_id: &str, entry_id: &str) -> Result<LorebookEntry, String> {
    Request::get(&format!("/api/lorebooks/{}/entries/{}", lorebook_id, entry_id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_lorebook_entry(input: &CreateLorebookEntryInput) -> Result<LorebookEntry, String> {
    Request::post(&format!("/api/lorebooks/{}/entries", input.lorebook_id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_lorebook_entry(entry_id: &str, input: &CreateLorebookEntryInput) -> Result<(), String> {
    Request::put(&format!("/api/lorebooks/{}/entries/{}", input.lorebook_id, entry_id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn delete_lorebook_entry(lorebook_id: &str, entry_id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/lorebooks/{}/entries/{}", lorebook_id, entry_id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn set_character_lorebooks(character_id: &str, lorebook_ids: Vec<String>) -> Result<(), String> {
    Request::put(&format!("/api/characters/{}/lorebooks", character_id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&SetLorebooksRequest { lorebook_ids })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn set_chat_lorebooks(chat_id: &str, lorebook_ids: Vec<String>) -> Result<(), String> {
    Request::put(&format!("/api/chats/{}/lorebooks", chat_id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&SetLorebooksRequest { lorebook_ids })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn get_character_lorebooks(character_id: &str) -> Result<Vec<String>, String> {
    Request::get(&format!("/api/characters/{}/lorebooks", character_id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Vec<String>>()
        .await
        .map_err(|e| e.to_string())
}

pub async fn get_chat_lorebooks(chat_id: &str) -> Result<Vec<String>, String> {
    Request::get(&format!("/api/chats/{}/lorebooks", chat_id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<Vec<String>>()
        .await
        .map_err(|e| e.to_string())
}

fn lorebook_entry_signature(
    name: &str, entry: &str, keywords: &str, priority: i64, weight: i64, enabled: bool,
    comment: &str, secondary_keys: &str, constant: bool, position: &str, probability: i64,
    use_probability: bool, selective: bool, selective_logic: i64, exclude_recursion: bool,
) -> String {
    format!(
        "{name}\u{1}{entry}\u{1}{keywords}\u{1}{priority}\u{1}{weight}\u{1}{enabled}\u{1}{comment}\u{1}{secondary_keys}\u{1}{constant}\u{1}{position}\u{1}{probability}\u{1}{use_probability}\u{1}{selective}\u{1}{selective_logic}\u{1}{exclude_recursion}"
    )
}

fn card_entry_signature(entry: &serde_json::Value) -> String {
    let ext = entry.get("extensions").cloned().unwrap_or(serde_json::json!({}));
    let keys = entry.get("keys").cloned().unwrap_or(serde_json::json!([]));
    let secondary_keys = entry.get("secondary_keys").cloned().unwrap_or(serde_json::json!([]));
    lorebook_entry_signature(
        entry.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        entry.get("content").and_then(|v| v.as_str()).unwrap_or(""),
        &keys.to_string(),
        entry.get("insertion_order").and_then(|v| v.as_i64()).unwrap_or(0),
        ext.get("weight").and_then(|v| v.as_i64()).unwrap_or(0),
        entry.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        entry.get("comment").and_then(|v| v.as_str()).unwrap_or(""),
        &secondary_keys.to_string(),
        entry.get("constant").and_then(|v| v.as_bool()).unwrap_or(false),
        entry.get("position").and_then(|v| v.as_str()).unwrap_or(""),
        ext.get("probability").and_then(|v| v.as_i64()).unwrap_or(100),
        ext.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(true),
        entry.get("selective").and_then(|v| v.as_bool()).unwrap_or(false),
        ext.get("selectiveLogic").and_then(|v| v.as_i64()).unwrap_or(0),
        ext.get("excludeRecursion").or_else(|| ext.get("exclude_recursion")).and_then(|v| v.as_bool()).unwrap_or(false),
    )
}

fn stored_entry_signature(e: &LorebookEntry) -> String {
    lorebook_entry_signature(
        &e.name, &e.entry, &e.keywords, e.priority, e.weight, e.enabled, &e.comment,
        &e.secondary_keys, e.constant, &e.position, e.probability, e.use_probability,
        e.selective, e.selective_logic, e.exclude_recursion,
    )
}

pub async fn find_matching_lorebook(input: &CreateLorebookInput, card_entries: &[serde_json::Value]) -> Option<String> {
    let existing = list_lorebooks().await.ok()?;
    let incoming: std::collections::HashSet<String> = card_entries.iter().map(card_entry_signature).collect();

    for candidate in existing.iter().filter(|l| l.name == input.name) {
        if candidate.description != input.description.clone().unwrap_or_default()
            || candidate.scan_depth != input.scan_depth.unwrap_or(0)
            || candidate.token_budget != input.token_budget.unwrap_or(0)
            || candidate.recursive_scanning != input.recursive_scanning.unwrap_or(false)
            || candidate.extensions != input.extensions.clone().unwrap_or_default()
        {
            continue;
        }
        let Ok(candidate_entries) = list_lorebook_entries(&candidate.id).await else { continue };
        let candidate_signatures: std::collections::HashSet<String> =
            candidate_entries.iter().map(stored_entry_signature).collect();
        if candidate_signatures == incoming {
            return Some(candidate.id.clone());
        }
    }
    None
}
