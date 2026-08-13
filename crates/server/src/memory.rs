use crate::provider::prompt::{estimate_tokens, ChatMessage};
use crate::provider::prompt::Role;
use crate::state::AppState;
use futures_util::StreamExt;

/// how much new unsummarized convo before we bother running another summary
/// pass. roughly SillyTavern's default interval but in tokens not messages,
/// scales better with chatty characters
const SUMMARY_TRIGGER_TOKENS: usize = 3000;
const SUMMARY_TARGET_WORDS: usize = 250;

fn format_entry(name: &str, content: &str) -> String {
    format!("{}:\n{}\n", name, content)
}

/// folds newly-accumulated chat history into the chat's running "story so
/// far" summary, once enough new content has piled up since the last pass.
/// called fire-and-forget after a character reply is saved; failures are
/// swallowed since this is a background quality-of-life feature, not part of
/// the reply the user is waiting on.
pub async fn maybe_update_chat_summary(state: &AppState, user_id: i64, chat_id: &str) {
    // serializes overlapping passes for this chat - see AppState::chat_summary_lock
    let lock = state.chat_summary_lock(chat_id).await;
    let _guard = lock.lock().await;

    let Some(chat) = crate::models::chat::get(&state.db.read_pool, user_id, chat_id).await.ok().flatten() else {
        return;
    };

    let tree = match crate::models::message::tree_for_chat(&state.db.read_pool, user_id, chat_id).await {
        Ok(t) => t,
        Err(_) => return,
    };
    let branch = crate::models::message::active_branch(&tree);
    let visible: Vec<_> = branch.iter().filter(|m| m.visible && !m.deleted).cloned().collect();

    let start_index = match &chat.memory_summary_message_id {
        Some(id) => visible.iter().position(|m| &m.id == id).map(|i| i + 1).unwrap_or(0),
        None => 0,
    };
    let new_messages = &visible[start_index..];
    if new_messages.is_empty() {
        return;
    }

    // 1:1 chat: one fixed character name for every assistant line, exactly
    // as before. group chat: no single name, resolved per message below.
    let fixed_character_name = match &chat.character_id {
        Some(character_id) => match crate::models::character::get(&state.db.read_pool, user_id, character_id).await {
            Ok(Some(c)) => Some(c.name),
            _ => return,
        },
        None => None,
    };

    let mut group_member_names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(group_id) = &chat.group_id {
        let members = match crate::models::group::list_members(&state.db.read_pool, group_id).await {
            Ok(m) => m,
            Err(_) => return,
        };
        for member in members {
            if let Ok(Some(c)) = crate::models::character::get(&state.db.read_pool, user_id, &member.character_id).await {
                group_member_names.insert(c.id, c.name);
            }
        }
    }

    let user = match crate::models::user::find_by_id(&state.db.read_pool, user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!(error = %e, "failed to find user for summary");
            return;
        }
    };
    let user_name = user
        .display_name.clone()
        .or_else(|| Some(user.username.clone()))
        .unwrap_or_default();

    let name_for = |role: &str, character_id: &Option<String>| -> String {
        if role != "assistant" {
            return user_name.clone();
        }
        if let Some(name) = &fixed_character_name {
            return name.clone();
        }
        character_id.as_ref()
            .and_then(|id| group_member_names.get(id))
            .cloned()
            .unwrap_or_else(|| "Someone".to_string())
    };

    let full_buffer: String = new_messages
        .iter()
        .map(|m| format_entry(&name_for(&m.role, &m.character_id), &m.content))
        .collect::<Vec<_>>()
        .join("\n");

    if estimate_tokens(&full_buffer) < SUMMARY_TRIGGER_TOKENS {
        return;
    }

    let cfg = match crate::models::settings::get_summary_config(&state.db.read_pool, user_id, &state.encryption_key).await {
        Ok(c) => c,
        Err(_) => return,
    };

    let instruction = format!(
        "Summarize the important facts, events, and character developments from the story so far, in {} words or less. If a previous summary is included below, use it as the base and fold the new events into it rather than repeating it verbatim. Respond with nothing but the summary itself.",
        SUMMARY_TARGET_WORDS
    );
    let prev_summary = chat.memory_summary.clone().unwrap_or_default();
    let mut fixed_tokens = estimate_tokens(&instruction) + estimate_tokens("New events:\n");
    if !prev_summary.is_empty() {
        fixed_tokens += estimate_tokens("Previous summary:\n") + estimate_tokens(&prev_summary);
    }

    // fold in new messages oldest-first, stopping once the summarization
    // model's own context limit (if capped, separately from the main
    // provider's) would be exceeded. whatever doesn't fit this pass is left
    // pending for the next one, since `memory_summary_message_id` only
    // advances to the last message actually folded in.
    let mut included: Vec<&crate::models::message::MessageNode> = Vec::new();
    let mut running_tokens = fixed_tokens;
    for m in new_messages {
        let entry_tokens = estimate_tokens(&format_entry(&name_for(&m.role, &m.character_id), &m.content));
        if cfg.context_limit > 0 && !included.is_empty() && running_tokens + entry_tokens > cfg.context_limit as usize {
            break;
        }
        running_tokens += entry_tokens;
        included.push(m);
    }
    if included.is_empty() {
        return;
    }

    let buffer: String = included
        .iter()
        .map(|m| format_entry(&name_for(&m.role, &m.character_id), &m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let mut summary_input = String::new();
    if !prev_summary.is_empty() {
        summary_input.push_str("Previous summary:\n");
        summary_input.push_str(&prev_summary);
        summary_input.push_str("\n\n");
    }
    summary_input.push_str("New events:\n");
    summary_input.push_str(&buffer);

    let messages = vec![
        ChatMessage { role: Role::System, content: instruction },
        ChatMessage { role: Role::User, content: summary_input },
    ];

    let provider: Box<dyn crate::provider::ModelProvider> = match cfg.provider_type.as_str() {
        "anthropic" => Box::new(crate::provider::AnthropicProvider),
        "gemini" => Box::new(crate::provider::GeminiProvider),
        "horde" => Box::new(crate::provider::HordeProvider),
        "novelai" => Box::new(crate::provider::NovelProvider),
        "openai" | "kobold" | "mancer" => Box::new(crate::provider::OpenAIProvider),
        other => {
            // same swallow-failures contract as the rest of this function,
            // just logged instead of quietly misrouted to OpenAI's API
            // with whatever key/url happens to be lying around
            tracing::error!("unrecognized provider_type '{other}' in summary settings, skipping summary update");
            return;
        }
    };

    let stream = provider
        .stream_completion(state.http_client.clone(), cfg.api_base_url, cfg.api_key, cfg.model_name, messages, crate::provider::SamplingParams::default())
        .await;
    let pieces: Vec<String> = stream.filter_map(|r| async move { r.ok() }).collect().await;
    let (summary, _) = crate::reasoning::extract_thinking(&pieces.concat());
    let summary = summary.trim().to_string();
    if summary.is_empty() {
        return;
    }

    let last_included_id = included.last().map(|m| m.id.clone()).unwrap_or_default();
    let _ = state
        .db
        .writer
        .update_chat_memory(user_id, chat_id.to_string(), summary, last_included_id)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_entry_puts_the_given_name_on_its_own_line() {
        assert_eq!(format_entry("Aria", "hello"), "Aria:\nhello\n");
    }
}
