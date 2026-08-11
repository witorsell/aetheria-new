use gloo_net::http::Request;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Clone, Default)]
pub struct Chat {
    pub id: String,
    pub character_id: Option<String>,
    pub group_id: Option<String>,
    pub title: String,
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct MessageNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub visible: bool,
    pub deleted: bool,
    pub created_at: i64,
    #[serde(default)]
    pub children: Vec<String>,
    pub raw_prompt: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub context_limit: Option<i64>,
    pub character_id: Option<String>,
}

#[derive(Deserialize, Clone, Default)]
pub struct MessageTree {
    pub root_id: Option<String>,
    pub messages: HashMap<String, MessageNode>,
    pub character: Option<super::characters::Character>,
    pub group: Option<super::groups::GroupWithMembers>,
}

#[derive(Deserialize, Clone)]
pub struct PromptMessage {
    pub role: String,
    pub content: String,
}

pub fn parse_raw_prompt(raw_prompt: &str) -> Vec<PromptMessage> {
    serde_json::from_str(raw_prompt).unwrap_or_default()
}

#[derive(Serialize)]
struct CreateChatInput<'a> {
    title: &'a str,
}

#[derive(Serialize)]
struct AddChatMemberRequest<'a> {
    character_id: &'a str,
}

pub async fn get_chat(id: &str) -> Result<Chat, String> {
    Request::get(&format!("/api/chats/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_chats(character_id: &str) -> Result<Vec<Chat>, String> {
    Request::get(&format!("/api/characters/{character_id}/chats"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn create_chat(character_id: &str, title: &str) -> Result<Chat, String> {
    Request::post(&format!("/api/characters/{character_id}/chats"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&CreateChatInput { title })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn add_chat_member(chat_id: &str, character_id: &str) -> Result<Chat, String> {
    Request::post(&format!("/api/chats/{chat_id}/members"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&AddChatMemberRequest { character_id })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn remove_chat_member(chat_id: &str, character_id: &str) -> Result<Chat, String> {
    let response = Request::delete(&format!("/api/chats/{chat_id}/members/{character_id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.ok() {
        return Err(response.text().await.unwrap_or_default());
    }
    response.json().await.map_err(|e| e.to_string())
}

pub async fn list_messages(chat_id: &str) -> Result<MessageTree, String> {
    Request::get(&format!("/api/chats/{chat_id}/messages"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn active_branch(chat_id: &str) -> Result<Vec<MessageNode>, String> {
    Request::get(&format!("/api/chats/{chat_id}/active_branch"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn subtree(
    chat_id: &str,
    from: &str,
    depth: usize,
) -> Result<HashMap<String, MessageNode>, String> {
    Request::get(&format!("/api/chats/{chat_id}/messages/tree?from={from}&depth={depth}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn delete_message(id: &str) -> Result<(), String> {
    Request::delete(&format!("/api/messages/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub async fn edit_message(id: &str, content: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    let body = serde_json::json!({ "content": content }).to_string();
    let init = web_sys::RequestInit::new();
    init.set_method("PATCH");
    init.set_credentials(web_sys::RequestCredentials::Include);
    init.set_body(&JsValue::from_str(&body));
    let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
    headers.set("Content-Type", "application/json").map_err(|e| format!("{e:?}"))?;
    init.set_headers(&headers);

    let request = web_sys::Request::new_with_str_and_init(
        &format!("/api/messages/{id}"),
        &init,
    )
    .map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or("no window")?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let response: web_sys::Response = response_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if response.ok() { Ok(()) } else { Err("failed to edit message".to_string()) }
}

pub async fn set_message_visibility(id: &str, visible: bool) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    let body = serde_json::json!({ "visible": visible }).to_string();
    let init = web_sys::RequestInit::new();
    init.set_method("PUT");
    init.set_credentials(web_sys::RequestCredentials::Include);
    init.set_body(&JsValue::from_str(&body));
    let headers = web_sys::Headers::new().map_err(|e| format!("{e:?}"))?;
    headers.set("Content-Type", "application/json").map_err(|e| format!("{e:?}"))?;
    init.set_headers(&headers);

    let request = web_sys::Request::new_with_str_and_init(
        &format!("/api/messages/{id}/visible"),
        &init,
    )
    .map_err(|e| format!("{e:?}"))?;
    let window = web_sys::window().ok_or("no window")?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let response: web_sys::Response = response_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if response.ok() { Ok(()) } else { Err("failed to toggle visibility".to_string()) }
}
