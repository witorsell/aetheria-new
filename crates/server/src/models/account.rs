use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct CharacterExport {
    pub id: String,
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_message: String,
    pub sample_chat: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub prefill: String,
    pub insert_depth_prompt: String,
    pub insert_depth: i32,
    pub talkativeness: f64,
    pub persona: String,
    pub extensions: String,
    pub folder_name: Option<String>,
    pub tags: Vec<String>,
    pub alternate_greetings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct LorebookEntryExport {
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

#[derive(Serialize, Deserialize)]
pub struct LorebookExport {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scan_depth: i64,
    pub token_budget: i64,
    pub recursive_scanning: bool,
    pub extensions: String,
    pub entries: Vec<LorebookEntryExport>,
}

#[derive(Serialize, Deserialize)]
pub struct GroupMemberExport {
    pub character_id: String,
    pub position: i64,
    pub disabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct GroupExport {
    pub id: String,
    pub name: String,
    pub activation_strategy: String,
    pub members: Vec<GroupMemberExport>,
}

#[derive(Serialize, Deserialize)]
pub struct MessageExport {
    pub id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub visible: bool,
    pub raw_prompt: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub context_limit: Option<i64>,
    pub character_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatExport {
    pub character_id: Option<String>,
    pub group_id: Option<String>,
    pub title: String,
    pub lorebook_ids: Vec<String>,
    pub messages: Vec<MessageExport>,
}

#[derive(Serialize, Deserialize)]
pub struct AccountExport {
    pub exported_at: String,
    pub characters: Vec<CharacterExport>,
    pub lorebooks: Vec<LorebookExport>,
    pub groups: Vec<GroupExport>,
    pub chats: Vec<ChatExport>,
    pub presets: Vec<crate::models::preset::PresetExport>,
    pub regex_scripts: Vec<crate::models::regex_script::RegexScriptInput>,
    pub themes: Vec<crate::models::theme::ThemeExport>,
}

pub async fn export_all(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<AccountExport> {
    // id -> name maps, built once, used to resolve folder/tag references
    // on characters below
    let folders = crate::models::character::list_folders(pool, user_id).await?;
    let folder_names: HashMap<String, String> =
        folders.into_iter().map(|f| (f.id, f.name)).collect();
    let tags = crate::models::character::list_tags(pool, user_id).await?;
    let tag_names: HashMap<String, String> = tags.into_iter().map(|t| (t.id, t.name)).collect();

    let characters = crate::models::character::list(pool, user_id).await?;
    let mut character_exports = Vec::with_capacity(characters.len());
    for c in &characters {
        let greetings = crate::models::character::list_alternate_greetings(pool, user_id, &c.id)
            .await?
            .into_iter()
            .map(|g| g.greeting)
            .collect();
        let tag_ids = crate::models::character::list_character_tags(pool, user_id, &c.id).await?;
        let tags = tag_ids
            .into_iter()
            .filter_map(|id| tag_names.get(&id).cloned())
            .collect();
        character_exports.push(CharacterExport {
            id: c.id.clone(),
            name: c.name.clone(),
            description: c.description.clone(),
            personality: c.personality.clone(),
            scenario: c.scenario.clone(),
            first_message: c.first_message.clone(),
            sample_chat: c.sample_chat.clone(),
            system_prompt: c.system_prompt.clone(),
            post_history_instructions: c.post_history_instructions.clone(),
            prefill: c.prefill.clone(),
            insert_depth_prompt: c.insert_depth_prompt.clone(),
            insert_depth: c.insert_depth,
            talkativeness: c.talkativeness,
            persona: c.persona.clone(),
            extensions: c.extensions.clone(),
            folder_name: c.folder_id.as_ref().and_then(|id| folder_names.get(id).cloned()),
            tags,
            alternate_greetings: greetings,
        });
    }

    let lorebooks = crate::models::lorebook::list(pool, user_id).await?;
    let mut lorebook_exports = Vec::with_capacity(lorebooks.len());
    for l in &lorebooks {
        let entries = crate::models::lorebook::list_entries(pool, user_id, &l.id)
            .await?
            .into_iter()
            .map(|e| LorebookEntryExport {
                name: e.name,
                entry: e.entry,
                keywords: e.keywords,
                priority: e.priority,
                weight: e.weight,
                enabled: e.enabled,
                comment: e.comment,
                secondary_keys: e.secondary_keys,
                constant: e.constant,
                position: e.position,
                probability: e.probability,
                use_probability: e.use_probability,
                selective: e.selective,
                selective_logic: e.selective_logic,
                exclude_recursion: e.exclude_recursion,
            })
            .collect();
        lorebook_exports.push(LorebookExport {
            id: l.id.clone(),
            name: l.name.clone(),
            description: l.description.clone(),
            scan_depth: l.scan_depth,
            token_budget: l.token_budget,
            recursive_scanning: l.recursive_scanning,
            extensions: l.extensions.clone(),
            entries,
        });
    }

    let groups = crate::models::group::list(pool, user_id).await?;
    let mut group_exports = Vec::with_capacity(groups.len());
    for g in &groups {
        let members = crate::models::group::list_members(pool, &g.id)
            .await?
            .into_iter()
            .map(|m| GroupMemberExport {
                character_id: m.character_id,
                position: m.position,
                disabled: m.disabled,
            })
            .collect();
        group_exports.push(GroupExport {
            id: g.id.clone(),
            name: g.name.clone(),
            activation_strategy: g.activation_strategy.clone(),
            members,
        });
    }

    let chats = crate::models::chat::list_all(pool, user_id).await?;
    let mut chat_exports = Vec::with_capacity(chats.len());
    for chat in &chats {
        let messages = sqlx::query_as::<_, crate::models::message::Message>(
            "SELECT * FROM messages WHERE chat_id = ? AND user_id = ? AND deleted = 0 ORDER BY created_at ASC",
        )
        .bind(&chat.id)
        .bind(user_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|m| MessageExport {
            id: m.id,
            parent_id: m.parent_id,
            role: m.role,
            content: m.content,
            visible: m.visible,
            raw_prompt: m.raw_prompt,
            prompt_tokens: m.prompt_tokens,
            context_limit: m.context_limit,
            character_id: m.character_id,
        })
        .collect();
        let lorebook_ids = crate::models::lorebook::list_chat_lorebooks(pool, user_id, &chat.id).await?;
        chat_exports.push(ChatExport {
            character_id: chat.character_id.clone(),
            group_id: chat.group_id.clone(),
            title: chat.title.clone(),
            lorebook_ids,
            messages,
        });
    }

    let presets = crate::models::preset::list(pool, user_id)
        .await?
        .into_iter()
        .map(|p| crate::models::preset::PresetExport {
            name: p.name,
            prompts: p.prompts,
            prompt_order: p.prompt_order,
        })
        .collect();

    let regex_scripts = crate::models::regex_script::list(pool, user_id)
        .await?
        .into_iter()
        .map(crate::models::regex_script::RegexScriptInput::from)
        .collect();

    let themes = crate::models::theme::list(pool, user_id)
        .await?
        .into_iter()
        .map(|t| crate::models::theme::ThemeExport { name: t.name, tokens: t.tokens })
        .collect();

    Ok(AccountExport {
        exported_at: chrono_like_now_iso8601(),
        characters: character_exports,
        lorebooks: lorebook_exports,
        groups: group_exports,
        chats: chat_exports,
        presets,
        regex_scripts,
        themes,
    })
}

fn chrono_like_now_iso8601() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // no chrono dependency in this crate - a millis-since-epoch string is
    // enough for a field that's only ever displayed, never parsed back
    format!("{millis}")
}
