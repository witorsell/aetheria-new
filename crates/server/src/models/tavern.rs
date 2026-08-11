use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct TavernCharacter {
    pub user_id: i64,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creator_notes: Option<String>,
    pub system_prompt: Option<String>,
    pub post_history_instructions: Option<String>,
    pub alternate_greetings: Option<Vec<String>>,
    pub character_book: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub creator: Option<String>,
    pub character_version: Option<String>,
    pub extensions: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
pub struct TavernV2Data {
    pub user_id: i64,
    pub spec: String,
    pub spec_version: String,
    pub data: TavernCharacter,
}
