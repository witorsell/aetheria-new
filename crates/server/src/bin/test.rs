use serde::Deserialize;

#[derive(Deserialize, Debug)]
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
}

fn main() {
    let json_str = r#"{"id":"d9761cc8-3615-4215-96a8-a71808fe5a69","name":"Robot","description":"","personality":"kind, compassionate, caring, tender, forgiving, enthusiastic","scenario":"{{char}} is sitting at a table in a busy cafe. You approach {{char}}'s table and wave at them. {{user}} sits down at the table in the chair opposite {{char}}.","first_message":"*A soft smile appears on {{char}}'s face as {{user}} enters the cafe and takes a seat* *Beep! Boop!* Hello, {{user}}! It's good to see you again. What would you like to chat about?","avatar_path":null,"created_at":1785704038535,"updated_at":1785753182692,"avatar_url":"/uploads/avatar_d9761cc8-3615-4215-96a8-a71808fe5a69.jpg","sample_chat":"","system_prompt":"","post_history_instructions":"","prefill":"","insert_depth_prompt":"","insert_depth":3,"persona":"{}","extensions":"{}","folder_id":null}"#;
    match serde_json::from_str::<Character>(json_str) {
        Ok(c) => println!("Success: {:?}", c),
        Err(e) => println!("Error: {}", e),
    }
}
