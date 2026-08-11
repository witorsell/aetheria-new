use gloo_net::http::Request;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub activation_strategy: String,
}

#[derive(Deserialize, Clone)]
pub struct GroupMember {
    pub character_id: String,
    pub position: i64,
    pub disabled: bool,
}

#[derive(Deserialize, Clone)]
pub struct GroupWithMembers {
    #[serde(flatten)]
    pub group: Group,
    pub members: Vec<GroupMember>,
}

#[derive(Serialize)]
struct GroupInput<'a> {
    name: &'a str,
    avatar_url: Option<&'a str>,
    activation_strategy: Option<&'a str>,
}

#[derive(Serialize)]
struct SetGroupMembersRequest {
    members: Vec<SetGroupMemberInput>,
}

#[derive(Serialize)]
struct SetGroupMemberInput {
    character_id: String,
    disabled: bool,
}

pub async fn get_group(id: &str) -> Result<GroupWithMembers, String> {
    Request::get(&format!("/api/groups/{id}"))
        .credentials(web_sys::RequestCredentials::Include)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn update_group(id: &str, name: &str, avatar_url: Option<&str>, activation_strategy: &str) -> Result<(), String> {
    Request::put(&format!("/api/groups/{id}"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&GroupInput { name, avatar_url, activation_strategy: Some(activation_strategy) })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| if r.ok() { Ok(()) } else { Err("failed to update group".to_string()) })
}

pub async fn set_group_members(group_id: &str, members: Vec<(String, bool)>) -> Result<(), String> {
    let body = SetGroupMembersRequest {
        members: members.into_iter().map(|(character_id, disabled)| SetGroupMemberInput { character_id, disabled }).collect(),
    };
    Request::put(&format!("/api/groups/{group_id}/members"))
        .header("Content-Type", "application/json")
        .credentials(web_sys::RequestCredentials::Include)
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| if r.ok() { Ok(()) } else { Err("failed to update group members".to_string()) })
}
