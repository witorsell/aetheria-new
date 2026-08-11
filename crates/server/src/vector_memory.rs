use crate::embedding;
use crate::state::AppState;

// caps texts per embeddings request, so a first-time backfill on a long
// chat doesn't send one giant batch
const INDEX_BATCH_SIZE: usize = 50;

fn thinking_stripped(role: &str, content: &str) -> String {
    if role == "assistant" {
        crate::reasoning::extract_thinking(content).0
    } else {
        content.to_string()
    }
}

// which messages survive prompt.rs's token-budget trim. context_limit
// <= 0 = unlimited, nothing's forgotten
fn messages_still_in_context<'a>(
    history: &'a [crate::models::message::MessageNode],
    context_limit: i64,
) -> std::collections::HashSet<&'a str> {
    if context_limit <= 0 {
        return history.iter().map(|m| m.id.as_str()).collect();
    }

    let mut protected = std::collections::HashSet::new();
    let mut used = 0usize;
    for m in history.iter().rev() {
        let tokens = crate::provider::prompt::estimate_tokens(&thinking_stripped(&m.role, &m.content));
        if used + tokens > context_limit as usize && !protected.is_empty() {
            break;
        }
        used += tokens;
        protected.insert(m.id.as_str());
    }
    protected
}

// embeds any un-vectored messages in the active branch, fire-and-forget
// after a reply saves. no-op with no embedding model configured
pub async fn maybe_index_chat(state: &AppState, user_id: i64, chat_id: &str) {
    let cfg = match crate::models::settings::get_embedding_config(&state.db.read_pool, user_id, &state.encryption_key).await {
        Ok(Some(c)) => c,
        _ => return,
    };

    let tree = match crate::models::message::tree_for_chat(&state.db.read_pool, user_id, chat_id).await {
        Ok(t) => t,
        Err(_) => return,
    };
    let branch = crate::models::message::active_branch(&tree);
    let visible: Vec<_> = branch.iter().filter(|m| m.visible && !m.deleted && !m.content.trim().is_empty()).cloned().collect();
    if visible.is_empty() {
        return;
    }

    let already_indexed = match crate::models::memory_chunk::existing_message_ids(&state.db.read_pool, user_id, chat_id).await {
        Ok(ids) => ids,
        Err(_) => return,
    };
    let pending: Vec<_> = visible.into_iter().filter(|m| !already_indexed.contains(&m.id)).collect();
    if pending.is_empty() {
        return;
    }

    for batch in pending.chunks(INDEX_BATCH_SIZE) {
        let texts: Vec<String> = batch.iter().map(|m| thinking_stripped(&m.role, &m.content)).collect();
        let vectors = match embedding::embed(&cfg, &texts, crate::embedding::EmbedMode::Document).await {
            Ok(v) => v,
            Err(_) => return,
        };
        for (message, (text, vector)) in batch.iter().zip(texts.into_iter().zip(vectors)) {
            let _ = state
                .db
                .writer
                .insert_memory_chunk(user_id, chat_id.to_string(), message.id.clone(), message.role.clone(), text, embedding::pack(&vector))
                .await;
        }
    }
}

// out-of-context messages relevant right now, one block for the lorebook
// injection. empty if the feature's off or nothing clears the threshold
pub async fn retrieve_relevant_context(
    state: &AppState,
    user_id: i64,
    chat_id: &str,
    history: &[crate::models::message::MessageNode],
    new_user_message: &str,
    character_name: &str,
    user_name: &str,
    context_limit: i64,
    speaker_names: Option<&std::collections::HashMap<String, String>>,
) -> String {
    let cfg = match crate::models::settings::get_embedding_config(&state.db.read_pool, user_id, &state.encryption_key).await {
        Ok(Some(c)) => c,
        _ => return String::new(),
    };

    let protected = messages_still_in_context(history, context_limit);
    if protected.len() >= history.len() {
        return String::new();
    }

    let chunks = match crate::models::memory_chunk::list_for_chat(&state.db.read_pool, user_id, chat_id).await {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    if chunks.is_empty() {
        return String::new();
    }

    let query_text: String = history
        .iter()
        .rev()
        .take(3)
        .map(|m| thinking_stripped(&m.role, &m.content))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .chain(std::iter::once(new_user_message.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    if query_text.trim().is_empty() {
        return String::new();
    }

    let query_vector = match embedding::embed(&cfg, &[query_text], crate::embedding::EmbedMode::Query).await {
        Ok(mut v) if !v.is_empty() => v.remove(0),
        _ => return String::new(),
    };

    let rag = match crate::models::settings::get_rag_params(&state.db.read_pool, user_id).await {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    // chunks can outlive the branch they were indexed from (a regenerate or
    // edit can abandon a message); only ones still on the active branch have
    // a meaningful position to sort by, so anything else is dropped rather
    // than surfacing content from a path the conversation no longer follows.
    let position_of: std::collections::HashMap<&str, usize> =
        history.iter().enumerate().map(|(i, m)| (m.id.as_str(), i)).collect();
    let character_id_of: std::collections::HashMap<&str, Option<&str>> =
        history.iter().map(|m| (m.id.as_str(), m.character_id.as_deref())).collect();

    let mut scored: Vec<(f32, &crate::models::memory_chunk::MemoryChunk)> = chunks
        .iter()
        .filter(|c| !protected.contains(c.message_id.as_str()) && position_of.contains_key(c.message_id.as_str()))
        .map(|c| (embedding::cosine_similarity(&query_vector, &embedding::unpack(&c.embedding)), c))
        .filter(|(score, _)| *score >= rag.score_threshold)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(rag.top_k);
    if scored.is_empty() {
        return String::new();
    }

    // chronological order reads more naturally than similarity order once
    // they're all sitting together in one block.
    scored.sort_by_key(|(_, c)| position_of[c.message_id.as_str()]);

    let name_for = |role: &str, message_id: &str| -> String {
        if role != "assistant" {
            return user_name.to_string();
        }
        match speaker_names {
            Some(names) => character_id_of.get(message_id)
                .and_then(|id| *id)
                .and_then(|id| names.get(id))
                .cloned()
                .unwrap_or_else(|| character_name.to_string()),
            None => character_name.to_string(),
        }
    };
    scored
        .iter()
        .map(|(_, c)| format!("{}: {}", name_for(&c.role, &c.message_id), c.text.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}
