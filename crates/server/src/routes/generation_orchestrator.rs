use crate::provider::prompt::{build_messages, estimate_message_tokens, ChatMessage, PromptContext, Role};
use crate::provider::ProviderError;
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::convert::Infallible;

// regenerate has two return paths on a group chat (the boxed group stream) vs
// a 1:1 chat (whatever run_generation returns), and impl trait requires every
// return path to resolve to the same concrete type. naming the type both
// functions already produce via .boxed().keep_alive(...) sidesteps that
pub(crate) type EventStream = axum::response::sse::KeepAliveStream<
    std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<Event, Infallible>> + Send>>,
>;

pub(crate) fn resolve_provider(provider_type: &str) -> Result<Box<dyn crate::provider::ModelProvider>, crate::error::ApiError> {
    match provider_type {
        "anthropic" => Ok(Box::new(crate::provider::AnthropicProvider)),
        "gemini" => Ok(Box::new(crate::provider::GeminiProvider)),
        "horde" => Ok(Box::new(crate::provider::HordeProvider)),
        "novelai" => Ok(Box::new(crate::provider::NovelProvider)),
        "openai" | "kobold" | "mancer" => Ok(Box::new(crate::provider::OpenAIProvider)),
        other => {
            tracing::error!("unrecognized provider_type '{other}' in settings");
            Err(StatusCode::UNPROCESSABLE_ENTITY.into())
        }
    }
}

/// everything needed to kick off `stream_completion` and later record what
/// was actually sent, assembled once so `run_generation`, `continue_generation`,
/// and `respond_as_user` don't each reimplement lorebook resolution, persona
/// lookup, and prompt assembly slightly differently (the three copies had
/// already drifted: only this version and two of the three call sites used
/// to apply the character's own system-prompt/post-history-instructions
/// override, the third silently didn't).
pub(crate) struct PreparedGeneration {
    pub provider: Box<dyn crate::provider::ModelProvider>,
    pub api_base_url: String,
    pub api_key: String,
    pub model_name: String,
    pub sampling: crate::provider::SamplingParams,
    pub messages: Vec<ChatMessage>,
    pub raw_prompt: String,
    pub prompt_tokens: i64,
    pub context_limit: i64,
}

pub(crate) async fn fetch_user_persona(
    pool: &sqlx::SqlitePool,
    user_id: i64,
) -> Result<(String, Option<String>), crate::error::ApiError> {
    let user = crate::models::user::find_by_id(pool, user_id).await?;
    let user_name = user
        .as_ref()
        .and_then(|u| u.display_name.clone())
        .unwrap_or_else(|| user.as_ref().map(|u| u.username.clone()).unwrap_or_default());
    let user_persona = user.as_ref().and_then(|u| {
        if u.use_persona {
            u.persona.clone()
        } else {
            None
        }
    });
    Ok((user_name, user_persona))
}

pub(crate) async fn resolve_lorebook_context(
    state: &AppState,
    user_id: i64,
    chat: &crate::models::chat::Chat,
    character: &crate::models::character::Character,
    history: &[crate::models::message::MessageNode],
    new_user_message: &str,
    user_name: &str,
    context_limit: i64,
    speaker_names: Option<&HashMap<String, String>>,
) -> Result<(String, String), crate::error::ApiError> {
    let mut all_lorebooks = Vec::new();
    let is_customized = sqlx::query_scalar::<_, bool>("SELECT lorebooks_customized FROM chats WHERE id = ?")
        .bind(&chat.id)
        .fetch_optional(&state.db.read_pool)
        .await
        .unwrap_or(Some(false))
        .unwrap_or(false);

    let mut lb_ids = if is_customized {
        crate::models::lorebook::list_chat_lorebooks(&state.db.read_pool, user_id, &chat.id).await?
    } else {
        crate::models::lorebook::list_character_lorebooks(&state.db.read_pool, user_id, &character.id).await?
    };
    lb_ids.sort();
    lb_ids.dedup();

    for lb_id in lb_ids {
        if let Ok(Some(lb)) = crate::models::lorebook::get(&state.db.read_pool, user_id, &lb_id).await {
            let entries = crate::models::lorebook::list_entries(&state.db.read_pool, user_id, &lb_id).await.unwrap_or_default();
            all_lorebooks.push((lb, entries));
        }
    }

    let (lorebook_before, lorebook_after) =
        crate::provider::prompt::scan_and_inject_lorebooks(&all_lorebooks, history, new_user_message);
    let lorebook_after = crate::provider::prompt::prepend_memory_summary(&lorebook_after, chat.memory_summary.as_deref());
    let vector_context = crate::vector_memory::retrieve_relevant_context(
        state, user_id, &chat.id, history, new_user_message, &character.name, user_name, context_limit, speaker_names,
    )
    .await;
    let lorebook_after = crate::provider::prompt::prepend_vector_context(&lorebook_after, &vector_context);

    Ok((lorebook_before, lorebook_after))
}

pub(crate) fn resolve_character_prompts(
    character: &crate::models::character::Character,
    settings: &crate::models::settings::SettingsView,
) -> (String, String) {
    let sys_prompt = if !character.system_prompt.trim().is_empty() {
        character.system_prompt.clone()
    } else {
        settings.system_prompt.clone()
    };
    let post_hist = if !character.post_history_instructions.trim().is_empty() {
        character.post_history_instructions.clone()
    } else {
        settings.post_history_instructions.clone()
    };
    (sys_prompt, post_hist)
}

pub(crate) async fn assemble_generation(
    state: &AppState,
    user_id: i64,
    chat: &crate::models::chat::Chat,
    character: &crate::models::character::Character,
    history: &[crate::models::message::MessageNode],
    new_user_message: &str,
    respond_as_user: bool,
    continuation: bool,
    speaker_names: Option<&HashMap<String, String>>,
    group_nudge: Option<&str>,
) -> Result<PreparedGeneration, crate::error::ApiError> {
    let settings = crate::models::settings::get_view(&state.db.read_pool, user_id).await?;
    let api_key =
        crate::models::settings::get_decrypted_api_key(&state.db.read_pool, user_id, &state.encryption_key).await?;

    let (user_name, user_persona) = fetch_user_persona(&state.db.read_pool, user_id).await?;

    let (lorebook_before, lorebook_after) = resolve_lorebook_context(
        state, user_id, chat, character, history, new_user_message, &user_name, settings.context_limit, speaker_names,
    )
    .await?;

    let regex_scripts = crate::models::regex_script::list_prompt_only(&state.db.read_pool, user_id).await.unwrap_or_default();
    let active_preset = match &settings.active_preset_id {
        Some(id) => crate::models::preset::get(&state.db.read_pool, user_id, id).await.ok().flatten(),
        None => None,
    };

    let (sys_prompt, post_hist) = resolve_character_prompts(character, &settings);

    let messages = build_messages(PromptContext {
        character,
        history,
        new_user_message,
        system_prompt_suffix: &sys_prompt,
        post_history_instructions: &post_hist,
        context_limit: settings.context_limit.max(0) as usize,
        user_name: &user_name,
        user_persona: user_persona.as_deref(),
        lorebook_before: &lorebook_before,
        lorebook_after: &lorebook_after,
        regex_scripts: &regex_scripts,
        active_preset: active_preset.as_ref(),
        respond_as_user,
        continuation,
        speaker_names,
        group_nudge,
    });
    let raw_prompt = serde_json::to_string(&messages).unwrap_or_default();
    let prompt_tokens = estimate_message_tokens(&messages) as i64;
    let provider = resolve_provider(&settings.provider_type)?;
    let sampling = settings.sampling_params();

    Ok(PreparedGeneration {
        provider,
        api_base_url: settings.api_base_url,
        api_key,
        model_name: settings.model_name,
        sampling,
        messages,
        raw_prompt,
        prompt_tokens,
        context_limit: settings.context_limit,
    })
}

// enabled members as activation candidates (position-ordered, list_members
// already sorts), plus everyone (enabled and disabled) in the character
// and name maps, since prefixing and the impersonation guard need disabled
// members' past lines/names too
pub(crate) async fn load_group_roster(
    state: &AppState, user_id: i64, group_id: &str,
) -> (Vec<crate::group_activation::ActivationCandidate>, HashMap<String, crate::models::character::Character>, HashMap<String, String>) {
    let members = match crate::models::group::list_members(&state.db.read_pool, group_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(group_id, error = %e, "failed to load group members");
            vec![]
        }
    };
    let mut candidates = Vec::new();
    let mut characters_by_id = HashMap::new();
    let mut all_names = HashMap::new();

    for member in members {
        if let Ok(Some(character)) = crate::models::character::get(&state.db.read_pool, user_id, &member.character_id).await {
            all_names.insert(character.id.clone(), character.name.clone());
            if !member.disabled {
                candidates.push(crate::group_activation::ActivationCandidate {
                    character_id: character.id.clone(),
                    name: character.name.clone(),
                    talkativeness: character.talkativeness,
                });
            }
            characters_by_id.insert(character.id.clone(), character);
        }
    }

    (candidates, characters_by_id, all_names)
}

// streams one reply per id in activated_ids, in order, each seeing the
// previous ones already in history. generate computes activated_ids via
// resolve_activation; regenerate just passes the one member being rerolled
// the generation loop (including every DB save) runs inside a spawned task
// instead of the stream generator itself, so it runs to completion even if
// the client disconnects mid-reply and axum stops polling the SSE body -
// otherwise a dropped connection loses whatever reply was already paid for
// to the provider, along with the save that would have recorded it. the
// stream returned here is just a subscriber over a channel the task feeds.
pub(crate) fn run_group_generation(
    state: AppState, user_id: i64, chat_id: String,
    chat: crate::models::chat::Chat,
    mut history: Vec<crate::models::message::MessageNode>,
    new_user_message: String,
    mut parent_id_for_reply: String,
    activated_ids: Vec<String>,
    characters_by_id: HashMap<String, crate::models::character::Character>,
    all_names: HashMap<String, String>,
) -> impl futures_util::Stream<Item = Result<Event, Infallible>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    tokio::spawn(async move {
        for character_id in activated_ids {
            let Some(character) = characters_by_id.get(&character_id) else { continue };
            let other_names: Vec<String> = all_names.iter()
                .filter(|(id, _)| id.as_str() != character_id)
                .map(|(_, name)| name.clone())
                .collect();
            let nudge = format!("[Write the next reply only as {}.]", character.name);

            let _ = tx.send(Ok(Event::default().event("member").data(
                serde_json::json!({ "character_id": character_id, "name": character.name }).to_string()
            ))).await;

            let prepared = match assemble_generation(
                &state, user_id, &chat, character, &history, &new_user_message, false, false,
                Some(&all_names), Some(&nudge),
            ).await {
                Ok(p) => p,
                Err(status) => {
                    let _ = tx.send(Ok(Event::default().event("error").data(format!("prompt assembly failed: {}", status.code)))).await;
                    continue;
                }
            };

            let mut provider_stream = prepared.provider.stream_completion(
                prepared.api_base_url, prepared.api_key, prepared.model_name, prepared.messages, prepared.sampling,
            ).await;

            let mut accumulated = String::new();
            while let Some(item) = provider_stream.next().await {
                match item {
                    Ok(delta) => {
                        accumulated.push_str(&delta);
                        let _ = tx.send(Ok(Event::default().data(delta))).await;
                    }
                    Err(ProviderError::Request(e)) => { let _ = tx.send(Ok(Event::default().event("error").data(e.to_string()))).await; }
                    Err(ProviderError::Status(code, body)) => { let _ = tx.send(Ok(Event::default().event("error").data(format!("{code}: {body}")))).await; }
                }
            }
            if accumulated.is_empty() {
                continue;
            }

            let cleaned = crate::group_activation::clean_group_reply(&accumulated, &other_names);

            let saved = state.db.writer.create_assistant_message_with_prompt(
                user_id, chat_id.clone(), Some(parent_id_for_reply.clone()), cleaned.clone(),
                prepared.raw_prompt, prepared.prompt_tokens, prepared.context_limit, Some(character_id.clone()),
            ).await;

            match saved {
                Ok(saved_msg) => {
                    parent_id_for_reply = saved_msg.id.clone();
                    history.push(crate::models::message::MessageNode {
                        user_id, id: saved_msg.id.clone(), parent_id: saved_msg.parent_id.clone(),
                        role: Role::Assistant.to_string(), content: cleaned, visible: true, deleted: false,
                        created_at: saved_msg.created_at, children: Vec::new(),
                        raw_prompt: saved_msg.raw_prompt.clone(), prompt_tokens: saved_msg.prompt_tokens,
                        context_limit: saved_msg.context_limit, character_id: Some(character_id.clone()),
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to save assistant message for {}", character_id);
                    let _ = tx.send(Ok(Event::default().event("error").data(format!("failed to save message: {}", e)))).await;
                }
            }
        }

        let state_for_memory = state.clone();
        let chat_id_for_memory = chat_id.clone();
        tokio::spawn(async move {
            crate::memory::maybe_update_chat_summary(&state_for_memory, user_id, &chat_id_for_memory).await;
            crate::vector_memory::maybe_index_chat(&state_for_memory, user_id, &chat_id_for_memory).await;
        });

        let _ = tx.send(Ok(Event::default().event("done").data(""))).await;
    });

    futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

pub(crate) async fn run_generation(
    state: AppState, user_id: i64,
    chat_id: String,
    branch_point: Option<String>,
) -> Result<Sse<EventStream>, crate::error::ApiError> {
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;

    let tree = crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?;
    let branch = match branch_point {
        Some(id) => crate::models::message::ancestor_path(&tree, &id).ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?,
        None => crate::models::message::active_branch(&tree),
    };
    let last_message = branch.last().ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?.clone();
    let new_user_message = last_message.content.clone();
    let history: Vec<_> = branch
        .iter()
        .filter(|m| m.visible && !m.deleted)
        .take(branch.len().saturating_sub(1))
        .cloned()
        .collect();

    if let Some(group_id) = chat.group_id.clone() {
        let group = crate::models::group::get(&state.db.read_pool, user_id, &group_id)
            .await?
            .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;
        let (candidates, characters_by_id, all_names) = load_group_roster(&state, user_id, &group_id).await;

        let last_speaker = history.iter().rev().find(|m| m.role == Role::Assistant.to_string()).and_then(|m| m.character_id.clone());
        let activated_ids = crate::group_activation::resolve_activation(
            &group.activation_strategy, &candidates, last_speaker.as_deref(), &new_user_message,
            &mut || rand::random::<f64>(),
        );

        // fold the real trigger message into history instead of passing it
        // as `new_user_message` to every activated member: build_messages
        // always appends `new_user_message` as the LAST turn, so on member
        // 2+ that would put the user's line after member 1's reply, when it
        // actually came before it. run_group_generation gets an empty
        // string instead, see build_history_messages's empty-check.
        let mut history_with_trigger = history;
        history_with_trigger.push(last_message.clone());

        let event_stream = run_group_generation(
            state, user_id, chat_id, chat, history_with_trigger, String::new(),
            last_message.id.clone(), activated_ids, characters_by_id, all_names,
        );
        return Ok(Sse::new(event_stream.boxed()).keep_alive(KeepAlive::default()));
    }

    let character_id = chat.character_id.clone().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let character = match crate::models::character::get(&state.db.read_pool, user_id, &character_id).await {
        Ok(Some(c)) => c,
        _ => return Err(StatusCode::NOT_FOUND.into()),
    };

    let prepared =
        assemble_generation(&state, user_id, &chat, &character, &history, &new_user_message, false, false, None, None).await?;

    let provider_stream = prepared
        .provider
        .stream_completion(prepared.api_base_url, prepared.api_key, prepared.model_name, prepared.messages, prepared.sampling)
        .await;

    let writer = state.db.writer.clone();
    let raw_prompt = prepared.raw_prompt;
    let prompt_tokens = prepared.prompt_tokens;
    let context_limit = prepared.context_limit;
    let parent_id_for_reply = last_message.id.clone();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    let chat_id_for_finish = chat_id.clone();
    let state_for_memory = state.clone();
    let chat_id_for_memory = chat_id.clone();
    // runs to completion in its own task regardless of whether the client
    // is still reading the SSE response - see the comment on
    // run_group_generation, this is the same fix for the 1:1 case
    tokio::spawn(async move {
        let mut provider_stream = provider_stream;
        let mut accumulated = String::new();
        while let Some(item) = provider_stream.next().await {
            let event = match item {
                Ok(delta) => {
                    accumulated.push_str(&delta);
                    Ok(Event::default().data(delta))
                }
                Err(ProviderError::Request(e)) => {
                    Ok(Event::default().event("error").data(e.to_string()))
                }
                Err(ProviderError::Status(code, body)) => {
                    Ok(Event::default().event("error").data(format!("{code}: {body}")))
                }
            };
            let _ = tx.send(event).await;
        }

        if !accumulated.is_empty() {
            let save_result = writer
                .create_assistant_message_with_prompt(user_id, chat_id_for_finish,
                    Some(parent_id_for_reply),
                    accumulated,
                    raw_prompt,
                    prompt_tokens,
                    context_limit,
                    None,
                )
                .await;
            match save_result {
                Ok(_) => {
                    tokio::spawn(async move {
                        crate::memory::maybe_update_chat_summary(&state_for_memory, user_id, &chat_id_for_memory).await;
                        crate::vector_memory::maybe_index_chat(&state_for_memory, user_id, &chat_id_for_memory).await;
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to save assistant message");
                    let _ = tx.send(Ok(Event::default().event("error").data(format!("failed to save message: {}", e)))).await;
                    return;
                }
            }
        }
        let _ = tx.send(Ok(Event::default().event("done").data(""))).await;
    });

    let event_stream = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });

    Ok(Sse::new(event_stream.boxed()).keep_alive(KeepAlive::default()))
}
