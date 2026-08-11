use crate::models::character::Character;
use crate::models::message::MessageNode;
use crate::models::lorebook::{Lorebook, LorebookEntry};
use crate::models::preset::Preset;
use crate::models::regex_script::RegexScript;
use crate::provider::regex_engine::apply_prompt_regex_scripts;
use serde::Serialize;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

impl From<&str> for Role {
    fn from(s: &str) -> Self {
        match s {
            "assistant" => Role::Assistant,
            "user" => Role::User,
            _ => Role::System,
        }
    }
}

impl From<String> for Role {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

// {{char}}/{{name}} -> character name, {{user}} -> logged-in user's name
pub(crate) fn substitute_macros(text: &str, char_name: &str, user_name: &str) -> String {
    let mut result = text.to_string();
    for (macro_str, replacement) in [
        ("{{char}}", char_name), ("{{Char}}", char_name), ("{{CHAR}}", char_name),
        ("{{name}}", char_name), ("{{Name}}", char_name), ("{{NAME}}", char_name),
        ("{char}", char_name), ("{Char}", char_name), ("{CHAR}", char_name),
        ("{name}", char_name), ("{Name}", char_name), ("{NAME}", char_name),
        ("{{user}}", user_name), ("{{User}}", user_name), ("{{USER}}", user_name),
        ("{user}", user_name), ("{User}", user_name), ("{USER}", user_name),
    ] {
        result = result.replace(macro_str, replacement);
    }
    result
}

// ~4 chars per token roughly

pub fn estimate_tokens(text: &str) -> usize {
    crate::tokenizer::count_tokens(text)
}

pub fn estimate_message_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(|m| estimate_tokens(&m.content) + 4).sum()
}

fn is_word_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() { return false; }
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let before_ok = if abs_pos == 0 {
            true
        } else {
            haystack[..abs_pos].chars().last().map_or(true, |c| !c.is_alphanumeric())
        };

        let after_pos = abs_pos + needle.len();
        let after_ok = if after_pos == haystack.len() {
            true
        } else {
            haystack[after_pos..].chars().next().map_or(true, |c| !c.is_alphanumeric())
        };

        if before_ok && after_ok {
            return true;
        }
        start = after_pos;
    }
    false
}

fn evaluate_entry(entry: &LorebookEntry, scan_text: &str) -> bool {
    if !entry.enabled {
        return false;
    }
    if entry.constant {
        return true;
    }
    // simple keyword matching for now (comma separated)
    if entry.keywords.is_empty() {
        return false;
    }
    let lower_scan = scan_text.to_lowercase();

    // parse keywords as a JSON array of strings, or fallback to comma-separated
    let keys: Vec<String> = serde_json::from_str(&entry.keywords).unwrap_or_else(|_| {
        entry.keywords.split(',').map(|s| s.trim().to_string()).collect()
    });

    for key in keys {
        let key = key.to_lowercase();
        if is_word_match(&lower_scan, &key) {
            return true;
        }
    }
    false
}

// activated entries split into before_char/after_char, sorted by priority
// desc then weight asc, deduped by id
pub fn scan_and_inject_lorebooks(
    lorebooks: &[(Lorebook, Vec<LorebookEntry>)],
    history: &[MessageNode],
    new_user_message: &str
) -> (String, String) {
    let mut activated_entries = Vec::new();

    for (lb, entries) in lorebooks {
        // collect scan text based on scan_depth
        let depth = if lb.scan_depth <= 0 { usize::MAX } else { lb.scan_depth as usize };

        let mut scan_text = String::new();
        for msg in history.iter().rev().take(depth) {
            scan_text.push_str(&msg.content);
            scan_text.push(' ');
        }
        scan_text.push_str(new_user_message);

        for entry in entries {
            if evaluate_entry(entry, &scan_text) {
                activated_entries.push(entry.clone());
            }
        }
    }

    // sort by priority desc, then weight asc
    activated_entries.sort_by(|a, b| {
        b.priority.cmp(&a.priority).then(a.weight.cmp(&b.weight))
    });

    // deduplicate by ID
    let mut seen = std::collections::HashSet::new();
    activated_entries.retain(|e| seen.insert(e.id.clone()));

    let mut before = String::new();
    let mut after = String::new();
    for entry in activated_entries {
        let bucket = if entry.position == "before_char" { &mut before } else { &mut after };
        bucket.push_str(&entry.entry);
        bucket.push('\n');
    }

    (before, after)
}

// folds the running memory summary into the after lorebook bucket, rides
// along right before chat history in both builders
pub fn prepend_memory_summary(lorebook_after: &str, memory_summary: Option<&str>) -> String {
    match memory_summary.filter(|s| !s.is_empty()) {
        Some(summary) => format!("[Story so far: {}]\n{}", summary, lorebook_after),
        None => lorebook_after.to_string(),
    }
}

// same idea as prepend_memory_summary but for retrieved vector/RAG memory
pub fn prepend_vector_context(lorebook_after: &str, vector_context: &str) -> String {
    if vector_context.trim().is_empty() {
        lorebook_after.to_string()
    } else {
        format!("[Relevant past moments:\n{}]\n{}", vector_context.trim(), lorebook_after)
    }
}

pub struct PromptContext<'a> {
    pub character: &'a Character,
    pub history: &'a [MessageNode],
    pub new_user_message: &'a str,
    pub system_prompt_suffix: &'a str,
    pub post_history_instructions: &'a str,
    pub context_limit: usize,
    pub user_name: &'a str,
    pub user_persona: Option<&'a str>,
    pub lorebook_before: &'a str,
    pub lorebook_after: &'a str,
    pub regex_scripts: &'a [RegexScript],
    pub active_preset: Option<&'a Preset>,
    // "respond as me": model writes the human's line, history roles swap
    pub respond_as_user: bool,
    // /continue: new_user_message is the character's own unfinished reply
    pub continuation: bool,
    // group chats only: character_id -> name, prefixes "{name}: " onto
    // assistant history lines (ST's names_behavior DEFAULT). None for 1:1.
    pub speaker_names: Option<&'a HashMap<String, String>>,
    // group chats only: appended as a system turn right before the
    // character's prefill (ST's groupNudge). None for 1:1.
    pub group_nudge: Option<&'a str>,
}

// history + new turn -> ChatMessages, with macros/reasoning-strip/regex
// applied. depth 0 = new user message, counts up back through history
fn build_history_messages(ctx: &PromptContext) -> (Vec<ChatMessage>, Option<ChatMessage>) {
    let character = ctx.character;
    let total = ctx.history.len();
    let history_messages: Vec<ChatMessage> = ctx.history
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let content = if node.role == "assistant" {
                crate::reasoning::extract_thinking(&node.content).0
            } else {
                node.content.clone()
            };
            let content = substitute_macros(&content, &character.name, ctx.user_name);
            let depth = (total - i) as i32;
            let content = apply_prompt_regex_scripts(ctx.regex_scripts, &content, &node.role, depth, &character.name, ctx.user_name);
            let content = if node.role == "assistant" {
                match ctx.speaker_names {
                    Some(names) => match node.character_id.as_deref().and_then(|id| names.get(id)) {
                        Some(speaker) => format!("{speaker}: {content}"),
                        None => content,
                    },
                    None => content,
                }
            } else {
                content
            };
            let role = if ctx.respond_as_user {
                if node.role == "assistant" { "user" } else { "assistant" }
            } else {
                node.role.as_str()
            };
            ChatMessage { role: role.into(), content }
        })
        .collect();

    let new_user_message = if ctx.respond_as_user {
        None
    } else if ctx.new_user_message.is_empty() {
        // group chats fold the real trigger message into `history` instead
        // (see run_generation's group branch) so it lands in the right
        // turn-order slot for every activated member, not just the first -
        // an empty `new_user_message` here means there's genuinely no
        // separate trailing turn to append.
        None
    } else {
        let role = if ctx.continuation { "assistant" } else { "user" };
        let new_user_content = substitute_macros(ctx.new_user_message, &character.name, ctx.user_name);
        let new_user_content = apply_prompt_regex_scripts(ctx.regex_scripts, &new_user_content, role, 0, &character.name, ctx.user_name);
        Some(ChatMessage { role: role.into(), content: new_user_content })
    };

    (history_messages, new_user_message)
}

pub fn build_messages(ctx: PromptContext) -> Vec<ChatMessage> {
    let mut messages = match ctx.active_preset {
        Some(preset) => build_preset_messages(&ctx, preset),
        None => build_legacy_messages(&ctx),
    };
    if ctx.respond_as_user {
        messages.push(ChatMessage {
            role: Role::System,
            content: format!(
                "Continue the scene by writing \"{}\"'s next message, not \"{}\"'s. Write only in \"{}\"'s voice, first person, covering their dialogue and actions only. Do not write any new dialogue, actions, or narration for \"{}\". Stop as soon as \"{}\"'s turn is finished.",
                ctx.user_name, ctx.character.name, ctx.user_name, ctx.character.name, ctx.user_name
            ),
        });
    }
    messages
}

fn build_legacy_messages(ctx: &PromptContext) -> Vec<ChatMessage> {
    let character = ctx.character;
    let user_name = ctx.user_name;
    let mut head = Vec::new();

    // 1. system prompt + character description + personality + scenario,
    //    all combined into a single system message.
    let mut system_content = String::new();
    if !ctx.system_prompt_suffix.is_empty() {
        system_content.push_str(&substitute_macros(ctx.system_prompt_suffix, &character.name, user_name));
    }
    if !character.description.is_empty() {
        if !system_content.is_empty() {
            system_content.push_str("\n\n");
        }
        system_content.push_str(&substitute_macros(&character.description, &character.name, user_name));
    }
    if !character.personality.is_empty() {
        if !system_content.is_empty() {
            system_content.push_str("\n\n");
        }
        system_content.push_str(&substitute_macros(&character.personality, &character.name, user_name));
    }
    if let Some(persona) = ctx.user_persona {
        if !system_content.is_empty() {
            system_content.push_str("\n\n");
        }
        system_content.push_str(&format!("\"{}\"'s Persona:\n{}", user_name, persona));
    }
    if !character.scenario.is_empty() {
        if !system_content.is_empty() {
            system_content.push_str("\n\n");
        }
        system_content.push_str(&substitute_macros(&character.scenario, &character.name, user_name));
    }
    if !system_content.is_empty() {
        head.push(ChatMessage {
            role: Role::System,
            content: system_content,
        });
    }

    // 4. Lorebook injections (activated entries go right after scenario, before history)
    let injected_lorebooks = format!("{}{}", ctx.lorebook_before, ctx.lorebook_after);
    if !injected_lorebooks.trim().is_empty() {
        head.push(ChatMessage {
            role: Role::System,
            content: injected_lorebooks.trim().to_string(),
        });
    }

    // 5. first message (only when no history yet)
    if ctx.history.is_empty() && !character.first_message.is_empty() {
        head.push(ChatMessage {
            role: if ctx.respond_as_user { Role::User } else { Role::Assistant },
            content: substitute_macros(&character.first_message, &character.name, user_name),
        });
    }

    // 6. chat history
    let (mut history_messages, new_user_chat) = build_history_messages(ctx);

    // inject insert_depth_prompt into history at the specified depth from the end
    if !character.insert_depth_prompt.is_empty() {
        let depth = character.insert_depth as usize;
        let insert_pos = if history_messages.len() > depth {
            history_messages.len() - depth
        } else {
            0
        };
        history_messages.insert(insert_pos, ChatMessage {
            role: Role::System,
            content: substitute_macros(&character.insert_depth_prompt, &character.name, user_name),
        });
    }

    // 7. tail: post_history_instructions then the new user message
    let mut tail = Vec::new();
    if !ctx.post_history_instructions.is_empty() {
        tail.push(ChatMessage {
            role: Role::System,
            content: substitute_macros(ctx.post_history_instructions, &character.name, user_name),
        });
    }
    if let Some(new_user_chat) = new_user_chat {
        tail.push(new_user_chat);
    }
    if let Some(nudge) = ctx.group_nudge {
        tail.push(ChatMessage { role: Role::System, content: nudge.to_string() });
    }

    // 8. prefill, seeds the assistant's reply. skipped for "respond as me"
    // (it's the character's own opening words, not the human's) and for
    // continuation (the unfinished reply itself is already the seed).
    if !character.prefill.is_empty() && !ctx.respond_as_user && !ctx.continuation {
        tail.push(ChatMessage {
            role: Role::Assistant,
            content: substitute_macros(&character.prefill, &character.name, user_name),
        });
    }

    // context trimming: drop oldest history first, keeping head + tail intact.
    // track a running total instead of re-tokenizing the whole buffer each
    // iteration, turning o(k*n) into o(n).
    if ctx.context_limit > 0 {
        let fixed_tokens = estimate_message_tokens(&head) + estimate_message_tokens(&tail);
        let mut history_tokens: usize = history_messages
            .iter()
            .map(|m| estimate_tokens(&m.content) + 4)
            .sum();
        while !history_messages.is_empty()
            && fixed_tokens + history_tokens > ctx.context_limit
        {
            let removed = history_messages.remove(0);
            history_tokens -= estimate_tokens(&removed.content) + 4;
        }
    }

    let mut messages = head;
    messages.extend(history_messages);
    messages.extend(tail);
    messages
}

// sample_chat -> system messages framed as hypothetical, not real history.
// <START>-split examples each get their own reset marker
fn build_dialogue_example_messages(sample_chat: &str, char_name: &str, user_name: &str) -> Vec<ChatMessage> {
    let raw = sample_chat.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let mut messages = Vec::new();
    messages.push(ChatMessage {
        role: Role::System,
        content: substitute_macros(
            "[The following are example excerpts of {{char}}'s speech and mannerisms, for reference only. \
             They are hypothetical and have NOT happened in the current conversation.]",
            char_name,
            user_name,
        ),
    });

    for (i, part) in raw.split("<START>").map(str::trim).filter(|p| !p.is_empty()).enumerate() {
        if i > 0 {
            messages.push(ChatMessage {
                role: Role::System,
                content: "New conversation started. Previous conversations are examples only.".to_string(),
            });
        }
        messages.push(ChatMessage {
            role: Role::System,
            content: substitute_macros(part, char_name, user_name),
        });
    }

    messages
}

// walks preset.prompt_order, resolving each entry's literal text or (for
// marker entries) live content, in order. ABSOLUTE entries splice in
// separately at their configured depth
fn build_preset_messages(ctx: &PromptContext, preset: &Preset) -> Vec<ChatMessage> {
    let character = ctx.character;
    let char_name = &character.name;
    let user_name = ctx.user_name;

    let (history_messages, new_user_chat) = build_history_messages(ctx);

    let by_id: HashMap<&str, &crate::models::preset::PresetPrompt> =
        preset.prompts.iter().map(|p| (p.identifier.as_str(), p)).collect();

    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut absolute_prompts: Vec<(i32, ChatMessage)> = Vec::new();
    let mut history_spliced = false;
    let mut history_start = 0usize;
    let mut history_len = 0usize;

    for order_entry in &preset.prompt_order {
        if !order_entry.enabled {
            continue;
        }
        let Some(prompt) = by_id.get(order_entry.identifier.as_str()) else {
            continue;
        };

        if prompt.marker {
            match prompt.identifier.as_str() {
                "worldInfoBefore" => {
                    let content = ctx.lorebook_before.trim();
                    if !content.is_empty() {
                        messages.push(ChatMessage { role: Role::System, content: content.to_string() });
                    }
                }
                "worldInfoAfter" => {
                    let content = ctx.lorebook_after.trim();
                    if !content.is_empty() {
                        messages.push(ChatMessage { role: Role::System, content: content.to_string() });
                    }
                }
                "charDescription" => {
                    let content = substitute_macros(&character.description, char_name, user_name);
                    if !content.is_empty() {
                        messages.push(ChatMessage { role: Role::System, content });
                    }
                }
                "charPersonality" => {
                    let content = substitute_macros(&character.personality, char_name, user_name);
                    if !content.is_empty() {
                        messages.push(ChatMessage { role: Role::System, content });
                    }
                }
                "scenario" => {
                    let content = substitute_macros(&character.scenario, char_name, user_name);
                    if !content.is_empty() {
                        messages.push(ChatMessage { role: Role::System, content });
                    }
                }
                "personaDescription" => {
                    if let Some(persona) = ctx.user_persona {
                        if !persona.is_empty() {
                            messages.push(ChatMessage {
                                role: Role::System,
                                content: format!("\"{}\"'s Persona:\n{}", user_name, persona),
                            });
                        }
                    }
                }
                "dialogueExamples" => {
                    messages.extend(build_dialogue_example_messages(&character.sample_chat, char_name, user_name));
                }
                "chatHistory" => {
                    history_start = messages.len();
                    messages.extend(history_messages.clone());
                    history_len = history_messages.len();
                    if let Some(new_user_chat) = &new_user_chat {
                        messages.push(new_user_chat.clone());
                        history_len += 1;
                    }
                    history_spliced = true;
                }
                _ => {}
            }
            continue;
        }

        let content = substitute_macros(&prompt.content, char_name, user_name);
        if content.is_empty() {
            continue;
        }
        let message = ChatMessage { role: prompt.role.as_str().into(), content };

        if prompt.injection_position == 1 {
            absolute_prompts.push((prompt.injection_depth, message));
        } else {
            messages.push(message);
        }
    }

    // no chatHistory in prompt_order? rare but valid, don't drop the convo
    if !history_spliced {
        history_start = messages.len();
        messages.extend(history_messages.clone());
        history_len = history_messages.len();
        if let Some(new_user_chat) = &new_user_chat {
            messages.push(new_user_chat.clone());
            history_len += 1;
        }
    }

    // context trimming: drop oldest history first (never the newest user
    // turn, the last item of the spliced range), keeping every other
    // assembled entry intact. running total avoids re-tokenizing the whole
    // message list every iteration.
    if ctx.context_limit > 0 {
        let mut total_tokens: usize = messages.iter().map(|m| estimate_tokens(&m.content) + 4).sum();
        while history_len > 1 && total_tokens > ctx.context_limit {
            let removed = messages.remove(history_start);
            total_tokens -= estimate_tokens(&removed.content) + 4;
            history_len -= 1;
        }
    }

    // absolute (depth-based) prompt injections, spliced in ascending depth
    // order the way `populationInjectionPrompts` processes depth 0..max.
    absolute_prompts.sort_by_key(|(depth, _)| *depth);
    for (depth, message) in absolute_prompts {
        let d = depth.max(0) as usize;
        let insert_pos = messages.len().saturating_sub(d);
        messages.insert(insert_pos, message);
    }

    // character-level insert_depth_prompt, same mechanic as legacy mode.
    if !character.insert_depth_prompt.is_empty() {
        let depth = character.insert_depth as usize;
        let insert_pos = messages.len().saturating_sub(depth);
        messages.insert(insert_pos, ChatMessage {
            role: Role::System,
            content: substitute_macros(&character.insert_depth_prompt, char_name, user_name),
        });
    }

    if let Some(nudge) = ctx.group_nudge {
        messages.push(ChatMessage { role: Role::System, content: nudge.to_string() });
    }

    if !character.prefill.is_empty() && !ctx.respond_as_user && !ctx.continuation {
        messages.push(ChatMessage {
            role: Role::Assistant,
            content: substitute_macros(&character.prefill, char_name, user_name),
        });
    }

    messages
}

#[cfg(test)]
mod dialogue_example_tests {
    use super::*;

    #[test]
    fn unstructured_sample_chat_gets_framed_as_hypothetical() {
        let sample = "*Utaha waves* Hi there, {{user}}!";
        let messages = build_dialogue_example_messages(sample, "Utaha", "Testuser");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert!(messages[0].content.contains("hypothetical"));
        assert!(messages[0].content.contains("NOT happened"));
        assert_eq!(messages[1].content, "*Utaha waves* Hi there, Testuser!");
    }

    #[test]
    fn start_markers_split_into_separate_examples_with_reset_between() {
        let sample = "<START>\nUtaha: Hi!\n<START>\nUtaha: Bye!";
        let messages = build_dialogue_example_messages(sample, "Utaha", "Testuser");
        // intro, example 1, reset marker, example 2
        assert_eq!(messages.len(), 4);
        assert!(messages[0].content.contains("hypothetical"));
        assert_eq!(messages[1].content, "Utaha: Hi!");
        assert_eq!(messages[2].content, "New conversation started. Previous conversations are examples only.");
        assert_eq!(messages[3].content, "Utaha: Bye!");
    }

    #[test]
    fn empty_sample_chat_produces_no_messages() {
        assert!(build_dialogue_example_messages("   ", "Utaha", "Testuser").is_empty());
    }
}

#[cfg(test)]
mod group_prompt_tests {
    use super::*;
    use crate::models::character::Character;

    fn character(name: &str) -> Character {
        Character {
            user_id: 1, id: "char-id".to_string(), name: name.to_string(),
            description: String::new(), personality: String::new(), scenario: String::new(),
            first_message: String::new(), avatar_path: None, avatar_url: None,
            sample_chat: String::new(), system_prompt: String::new(), post_history_instructions: String::new(),
            prefill: String::new(), insert_depth_prompt: String::new(), insert_depth: 3, talkativeness: 0.5,
            persona: "{}".to_string(), extensions: "{}".to_string(), folder_id: None,
            created_at: 0, updated_at: 0,
        }
    }

    fn node(id: &str, role: &str, content: &str, character_id: Option<&str>) -> MessageNode {
        MessageNode {
            user_id: 1, id: id.to_string(), parent_id: None, role: role.into(), content: content.to_string(),
            visible: true, deleted: false, created_at: 0, children: Vec::new(),
            raw_prompt: None, prompt_tokens: None, context_limit: None,
            character_id: character_id.map(str::to_string),
        }
    }

    #[test]
    fn group_history_gets_name_prefixed_by_actual_speaker_not_current_character() {
        let aria = character("Aria");
        let mut names = HashMap::new();
        names.insert("aria-id".to_string(), "Aria".to_string());
        names.insert("beck-id".to_string(), "Beck".to_string());

        let history = vec![
            node("1", "user", "hey both of you", None),
            node("2", "assistant", "hi!", Some("aria-id")),
            node("3", "assistant", "hello there", Some("beck-id")),
        ];

        let ctx = PromptContext {
            character: &aria, history: &history, new_user_message: "",
            system_prompt_suffix: "", post_history_instructions: "", context_limit: 0,
            user_name: "Testuser", user_persona: None, lorebook_before: "", lorebook_after: "",
            regex_scripts: &[], active_preset: None, respond_as_user: false, continuation: false,
            speaker_names: Some(&names),
            group_nudge: None,
        };
        let (messages, _) = build_history_messages(&ctx);

        assert_eq!(messages[0].content, "hey both of you", "user turns never get name-prefixed");
        assert_eq!(messages[1].content, "Aria: hi!", "prefixed even though Aria is the CURRENT ctx.character");
        assert_eq!(messages[2].content, "Beck: hello there", "prefixed with the actual historical speaker, not ctx.character");
    }

    #[test]
    fn one_on_one_history_is_never_prefixed_when_speaker_names_is_none() {
        let aria = character("Aria");
        let history = vec![node("1", "assistant", "hi!", None)];
        let ctx = PromptContext {
            character: &aria, history: &history, new_user_message: "",
            system_prompt_suffix: "", post_history_instructions: "", context_limit: 0,
            user_name: "Testuser", user_persona: None, lorebook_before: "", lorebook_after: "",
            regex_scripts: &[], active_preset: None, respond_as_user: false, continuation: false,
            speaker_names: None,
            group_nudge: None,
        };
        let (messages, _) = build_history_messages(&ctx);
        assert_eq!(messages[0].content, "hi!");
    }
}
