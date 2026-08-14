use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, PartialEq)]
pub struct Character {
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_message: String,
    pub avatar_url: Option<String>,
    pub sample_chat: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub prefill: String,
    pub insert_depth_prompt: String,
    pub insert_depth: i32,
    pub persona: String,
    pub extensions: String,
    pub folder_id: Option<String>,
    pub talkativeness: f64,
}

#[derive(Serialize)]
pub struct CharacterInput<'a> {
    pub name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_chat: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_history_instructions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_depth_prompt: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_depth: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persona: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub talkativeness: Option<f64>,
}

#[derive(Deserialize, Clone)]
pub struct AlternateGreeting {
    pub id: String,
    pub character_id: String,
    pub greeting: String,
    pub created_at: i64,
}

#[derive(Deserialize, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Deserialize, Clone)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
}

pub async fn list_characters() -> Result<Vec<Character>, String> {
    Request::get("/api/characters")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_character(input: CharacterInput<'_>) -> Result<Character, String> {
    Request::post("/api/characters")
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

pub async fn get_character(id: &str) -> Result<Character, String> {
    Request::get(&format!("/api/characters/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_character(id: &str, input: CharacterInput<'_>) -> Result<(), String> {
    Request::put(&format!("/api/characters/{id}"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&input)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| if r.ok() { Ok(()) } else { Err("failed to update character".to_string()) })
}

pub async fn delete_character(id: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("/api/characters/{}", id))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }
    Ok(())
}

pub async fn list_alternate_greetings(character_id: &str) -> Result<Vec<AlternateGreeting>, String> {
    Request::get(&format!("/api/characters/{character_id}/greetings"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn add_alternate_greeting(character_id: &str, greeting: &str) -> Result<AlternateGreeting, String> {
    Request::post(&format!("/api/characters/{character_id}/greetings"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&serde_json::json!({ "greeting": greeting }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_alternate_greeting(character_id: &str, greeting_id: &str, greeting: &str) -> Result<(), String> {
    Request::put(&format!("/api/characters/{}/greetings/{}", character_id, greeting_id))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&serde_json::json!({ "greeting": greeting }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn delete_alternate_greeting(character_id: &str, greeting_id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/characters/{character_id}/greetings/{greeting_id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn list_tags() -> Result<Vec<Tag>, String> {
    Request::get("/api/tags")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_tag(name: &str, color: Option<&str>) -> Result<Tag, String> {
    let mut body = serde_json::json!({ "name": name });
    if let Some(c) = color {
        body["color"] = serde_json::Value::String(c.to_string());
    }
    Request::post("/api/tags")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_tag(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/tags/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn get_character_tags(character_id: &str) -> Result<Vec<String>, String> {
    Request::get(&format!("/api/characters/{character_id}/tags"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_all_character_tags() -> Result<std::collections::HashMap<String, Vec<String>>, String> {
    Request::get("/api/character-tags")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn set_character_tags(character_id: &str, tag_ids: &[String]) -> Result<(), String> {
    Request::put(&format!("/api/characters/{character_id}/tags"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(tag_ids)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn list_folders() -> Result<Vec<Folder>, String> {
    Request::get("/api/folders")
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_folder(name: &str, parent_id: Option<&str>) -> Result<Folder, String> {
    let mut body = serde_json::json!({ "name": name });
    if let Some(p) = parent_id {
        body["parent_id"] = serde_json::Value::String(p.to_string());
    }
    Request::post("/api/folders")
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_folder(id: &str, name: &str, parent_id: Option<&str>) -> Result<(), String> {
    let mut body = serde_json::json!({ "name": name });
    if let Some(p) = parent_id {
        body["parent_id"] = serde_json::Value::String(p.to_string());
    }
    Request::put(&format!("/api/folders/{id}"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn delete_folder(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/folders/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn import_character(file: web_sys::File) -> Result<Character, String> {
    let form = web_sys::FormData::new().map_err(|_| "Failed to create FormData")?;
    form.append_with_blob("file", &file).map_err(|_| "Failed to append file")?;

    let resp = Request::post("/api/import/character")
        .credentials(web_sys::RequestCredentials::Include)
        .body(form.clone())
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err(resp.text().await.unwrap_or_default());
    }

    let json_val: serde_json::Value = resp.json().await.map_err(|e| format!("JSON parse error: {}", e))?;
    let data = json_val.get("data").unwrap_or(&json_val);

    let name = extract_card_field_str(data, &["name", "ch_name", "char_name", "title"]).unwrap_or("Imported Character");

    let card_desc = extract_card_field_str(data, &["description", "char_persona", "persona"]);
    let card_pers = extract_card_field_str(data, &["personality", "summary"]);
    let card_notes = extract_card_field_str(data, &["creator_notes", "creatorcomment", "comment", "notes", "author_notes"]);

    let personality_combined = match (card_desc, card_pers) {
        (Some(d), Some(p)) if d != p => format!("{d}\n\n{p}"),
        (Some(d), _) => d.to_string(),
        (None, Some(p)) => p.to_string(),
        (None, None) => String::new(),
    };
    let personality = if personality_combined.is_empty() { None } else { Some(personality_combined.as_str()) };

    let description = card_notes.or(if card_desc.is_some() { card_pers } else { None });
    let scenario = extract_card_field_str(data, &["scenario", "world_scenario"]);
    let first_message = extract_card_field_str(data, &["first_mes", "first_message", "greeting", "entry"]);
    let sample_chat = extract_card_field_str(data, &["mes_example", "example_dialogue", "definition", "examples"]);
    let system_prompt = extract_card_field_str(data, &["system_prompt", "system", "system_prompt_prefix"]);
    let post_history_instructions = extract_card_field_str(data, &["post_history_instructions", "post_history", "post_history_prompt"]);
    let extensions = data.get("extensions").map(|v| v.to_string());

    let input = CharacterInput {
        name,
        description,
        personality,
        scenario,
        first_message,
        avatar_url: None,
        sample_chat,
        system_prompt,
        post_history_instructions,
        prefill: None,
        insert_depth_prompt: None,
        insert_depth: None,
        persona: None,
        extensions: extensions.as_deref(),
        folder_id: None,
        talkativeness: None,
    };

    let chara = create_character(input).await?;

    match Request::post(&format!("/api/characters/{}/avatar", chara.id))
        .credentials(web_sys::RequestCredentials::Include)
        .body(form)
        .map_err(|e| e.to_string())?
        .send()
        .await
    {
        Ok(resp) if !resp.ok() => {
            let body = resp.text().await.unwrap_or_default();
            web_sys::console::warn_1(&format!("avatar upload failed during import: {body}").into());
        }
        Err(e) => web_sys::console::warn_1(&format!("avatar upload failed during import: {e}").into()),
        _ => {}
    }

    if let Some(greetings) = data.get("alternate_greetings").and_then(|v| v.as_array()) {
        for greeting in greetings.iter().filter_map(|v| v.as_str()).filter(|g| !g.is_empty()) {
            let _ = add_alternate_greeting(&chara.id, greeting).await;
        }
    }

    // character card v2 spec's `tags: string[]` - create_tag reuses an
    // existing tag of that name (case-insensitively) instead of erroring,
    // so re-importing the same card twice doesn't pile up duplicates. capped
    // like SillyTavern's own import (ANTI_TROLL_MAX_TAGS) since this list
    // comes from an untrusted file.
    const MAX_IMPORTED_TAGS: usize = 50;
    if let Some(card_tags) = data.get("tags").and_then(|v| v.as_array()) {
        let mut tag_ids = Vec::new();
        for name in card_tags.iter().filter_map(|v| v.as_str()).map(str::trim).filter(|n| !n.is_empty()).take(MAX_IMPORTED_TAGS) {
            if let Ok(tag) = create_tag(name, None).await {
                tag_ids.push(tag.id);
            }
        }
        if !tag_ids.is_empty() {
            let _ = set_character_tags(&chara.id, &tag_ids).await;
        }
    }

    if let Some(book) = data.get("character_book") {
        let no_entries = Vec::new();
        let entries = book.get("entries").and_then(|v| v.as_array()).unwrap_or(&no_entries);
        if !entries.is_empty() {
            let book_name = book
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{} Lorebook", name));
            let lorebook_input = super::lorebooks::CreateLorebookInput {
                name: book_name,
                description: book.get("description").and_then(|v| v.as_str()).map(str::to_string),
                scan_depth: book.get("scan_depth").and_then(|v| v.as_i64()),
                token_budget: book.get("token_budget").and_then(|v| v.as_i64()),
                recursive_scanning: book.get("recursive_scanning").and_then(|v| v.as_bool()),
                extensions: book.get("extensions").map(|v| v.to_string()),
            };

            let lorebook_id = match super::lorebooks::find_matching_lorebook(&lorebook_input, entries).await {
                Some(id) => Some(id),
                None => match super::lorebooks::create_lorebook(&lorebook_input).await {
                    Ok(lorebook) => {
                        for entry in entries {
                            let ext = entry.get("extensions").cloned().unwrap_or(serde_json::json!({}));
                            let keys = entry.get("keys").cloned().unwrap_or(serde_json::json!([]));
                            let secondary_keys = entry.get("secondary_keys").cloned().unwrap_or(serde_json::json!([]));
                            let entry_input = super::lorebooks::CreateLorebookEntryInput {
                                lorebook_id: lorebook.id.clone(),
                                name: entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                entry: entry.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                keywords: Some(keys.to_string()),
                                priority: entry.get("insertion_order").and_then(|v| v.as_i64()),
                                weight: ext.get("weight").and_then(|v| v.as_i64()),
                                enabled: entry.get("enabled").and_then(|v| v.as_bool()),
                                comment: entry.get("comment").and_then(|v| v.as_str()).map(str::to_string),
                                secondary_keys: Some(secondary_keys.to_string()),
                                constant: entry.get("constant").and_then(|v| v.as_bool()),
                                position: entry.get("position").and_then(|v| v.as_str()).map(str::to_string),
                                probability: Some(ext.get("probability").and_then(|v| v.as_i64()).unwrap_or(100)),
                                use_probability: Some(ext.get("useProbability").and_then(|v| v.as_bool()).unwrap_or(true)),
                                selective: entry.get("selective").and_then(|v| v.as_bool()),
                                selective_logic: ext.get("selectiveLogic").and_then(|v| v.as_i64()),
                                exclude_recursion: ext.get("excludeRecursion").or_else(|| ext.get("exclude_recursion")).and_then(|v| v.as_bool()),
                            };
                            let _ = super::lorebooks::create_lorebook_entry(&entry_input).await;
                        }
                        Some(lorebook.id)
                    }
                    Err(_) => None,
                },
            };

            if let Some(id) = lorebook_id {
                let _ = super::lorebooks::set_character_lorebooks(&chara.id, vec![id]).await;
            }
        }
    }

    Ok(chara)
}

fn extract_card_field_str<'a>(data: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    for &k in keys {
        if let Some(v) = data.get(k).and_then(|v| v.as_str()) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}
