use crate::provider::ProviderError;
use crate::state::AppState;
use crate::routes::generation_orchestrator::{resolve_provider, assemble_generation, run_generation, EventStream};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::StreamExt;
use serde::Deserialize;
use std::convert::Infallible;

use crate::provider::prompt::Role;

#[derive(Deserialize)]
pub struct GenerateInput {
    pub content: String,
    pub parent_id: Option<String>,
}

pub async fn generate(Extension(user_id): Extension<i64>, 
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    axum::Json(input): axum::Json<GenerateInput>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, crate::error::ApiError> {
    if input.content.trim().is_empty() {
        return Err(crate::error::ApiError::bad_request("Message content cannot be empty"));
    }
    state.check_generation_rate_limit(user_id).await?;
    let parent_id = input.parent_id.clone();
    let message = state
        .db
        .writer
        .create_message(user_id, chat_id.clone(), parent_id, Role::User.to_string(), input.content)
        .await
        .map_err(|_| crate::error::ApiError::internal("Failed to create message"))?;

    run_generation(state, user_id, chat_id, Some(message.id)).await
}

#[derive(Deserialize)]
pub struct RegenerateParams {
    pub parent_id: Option<String>,
    pub character_id: Option<String>,
}

pub async fn regenerate(Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(params): Query<RegenerateParams>,
) -> Result<Sse<EventStream>, crate::error::ApiError> {
    state.check_generation_rate_limit(user_id).await?;
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;

    if let Some(group_id) = chat.group_id.clone() {
        // a group reroll targets one named member, not the whole activation
        // strategy again, character_id says who's being rerolled
        let character_id = params.character_id.ok_or_else(|| crate::error::ApiError::from(StatusCode::BAD_REQUEST))?;

        let tree = crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?;
        let branch = match &params.parent_id {
            Some(id) => crate::models::message::ancestor_path(&tree, id).ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?,
            None => crate::models::message::active_branch(&tree),
        };
        let last_message = branch.last().ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?.clone();
        let history: Vec<_> = branch
            .iter()
            .filter(|m| m.visible && !m.deleted)
            .take(branch.len().saturating_sub(1))
            .cloned()
            .collect();

        let (_, characters_by_id, all_names) = crate::routes::generation_orchestrator::load_group_roster(&state, user_id, &group_id).await;
        if !characters_by_id.contains_key(&character_id) {
            return Err(StatusCode::NOT_FOUND.into());
        }

        // branching off another member's reply (not a fresh user message)
        // means last_message is an assistant node, fold it into history
        // same as the fresh-generate path does, so it's name-prefixed
        // instead of showing up as an unlabeled user turn
        let (history, trigger_text) = if last_message.role == Role::Assistant.to_string() {
            let mut history_with_trigger = history;
            history_with_trigger.push(last_message.clone());
            (history_with_trigger, String::new())
        } else {
            (history, last_message.content.clone())
        };

        let event_stream = crate::routes::generation_orchestrator::run_group_generation(
            state, user_id, chat_id, chat, history, trigger_text,
            last_message.id.clone(), vec![character_id], characters_by_id, all_names,
        );
        return Ok(Sse::new(event_stream.boxed()).keep_alive(KeepAlive::default()));
    }

    // parent_id, when given, is the user message to branch a new reply from
    // (a swipe/reroll); without it, fall back to whatever the active branch
    // currently ends on (a fresh user message awaiting its first reply).
    run_generation(state, user_id, chat_id, params.parent_id).await
}

pub async fn continue_generation(Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, crate::error::ApiError> {
    state.check_generation_rate_limit(user_id).await?;
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;
    if chat.group_id.is_some() {
        return Err(StatusCode::NOT_IMPLEMENTED.into());
    }
    let character_id = chat.character_id.clone().ok_or_else(|| crate::error::ApiError::from(StatusCode::UNPROCESSABLE_ENTITY))?;
    let character = match crate::models::character::get(&state.db.read_pool, user_id, &character_id).await {
        Ok(Some(c)) => c,
        _ => return Err(StatusCode::NOT_FOUND.into()),
    };

    let tree = crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?;
    let branch = crate::models::message::active_branch(&tree);
    let last = branch.last().ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;
    if last.role != Role::Assistant.to_string() || last.deleted {
        return Err(StatusCode::BAD_REQUEST.into());
    }

    // exclude the last assistant message from the prompt array (it's sent
    // separately as the continuation prefix, see `continuation: true` below).
    let history: Vec<_> = branch
        .iter()
        .filter(|m| m.visible && !m.deleted)
        .take(branch.len() - 1)
        .cloned()
        .collect();

    let prepared =
        assemble_generation(&state, user_id, &chat, &character, &history, &last.content, false, true, None, None).await?;

    let provider_stream = prepared
        .provider
        .stream_completion(state.http_client.clone(), prepared.api_base_url, prepared.api_key, prepared.model_name, prepared.messages, prepared.sampling)
        .await;

    let writer = state.db.writer.clone();
    let last_id = last.id.clone();
    let raw_prompt = prepared.raw_prompt;
    let prompt_tokens = prepared.prompt_tokens;
    let context_limit = prepared.context_limit;
    let accumulated = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let accumulated_for_stream = accumulated.clone();

    let sse_stream = provider_stream.then(move |item| {
        let accumulated = accumulated_for_stream.clone();
        async move {
            match item {
                Ok(delta) => {
                    accumulated.lock().await.push_str(&delta);
                    Ok(Event::default().data(delta))
                }
                Err(ProviderError::Request(e)) => {
                    Ok(Event::default().event("error").data(e.to_string()))
                }
                Err(ProviderError::Status(code, body)) => {
                    Ok(Event::default().event("error").data(format!("{code}: {body}")))
                }
            }
        }
    });

    let finishing_stream = sse_stream.chain(futures_util::stream::once(async move {
        let final_content = accumulated.lock().await.clone();
        if !final_content.is_empty() {
            if let Err(e) = writer
                .continue_message(user_id, last_id, final_content, raw_prompt, prompt_tokens, context_limit)
                .await
            {
                tracing::error!(error = %e, "failed to continue message");
            }
        }
        Ok(Event::default().event("done").data(""))
    }));

    Ok(Sse::new(finishing_stream).keep_alive(KeepAlive::default()))
}

#[derive(Deserialize)]
pub struct RespondAsUserParams {
    pub parent_id: Option<String>,
}

/// "respond as me": generates the human's next line, not the character's.
/// no fresh user turn here, whole active branch is history, PromptContext's
/// respond_as_user flag makes build_messages swap roles and skip the new
/// turn. saved as a user-role message so it shows up as your own line.
pub async fn respond_as_user(
    Extension(user_id): Extension<i64>,
    State(state): State<AppState>,
    Path(chat_id): Path<String>,
    Query(params): Query<RespondAsUserParams>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, crate::error::ApiError> {
    state.check_generation_rate_limit(user_id).await?;
    let chat = crate::models::chat::get(&state.db.read_pool, user_id, &chat_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;
    if chat.group_id.is_some() {
        return Err(StatusCode::NOT_IMPLEMENTED.into());
    }
    let character_id = chat.character_id.clone().ok_or_else(|| crate::error::ApiError::from(StatusCode::UNPROCESSABLE_ENTITY))?;
    let character = crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;

    let tree = crate::models::message::tree_for_chat(&state.db.read_pool, user_id, &chat_id).await?;
    let branch = match params.parent_id {
        Some(id) => crate::models::message::ancestor_path(&tree, &id).ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?,
        None => crate::models::message::active_branch(&tree),
    };
    let last = branch.last().ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;
    let parent_id_for_reply = last.id.clone();

    let history: Vec<_> = branch.iter().filter(|m| m.visible && !m.deleted).cloned().collect();

    let prepared = assemble_generation(&state, user_id, &chat, &character, &history, "", true, false, None, None).await?;

    let provider_stream = prepared
        .provider
        .stream_completion(state.http_client.clone(), prepared.api_base_url, prepared.api_key, prepared.model_name, prepared.messages, prepared.sampling)
        .await;

    let writer = state.db.writer.clone();
    let raw_prompt = prepared.raw_prompt;
    let prompt_tokens = prepared.prompt_tokens;
    let context_limit = prepared.context_limit;
    let accumulated = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
    let accumulated_for_stream = accumulated.clone();

    let sse_stream = provider_stream.then(move |item| {
        let accumulated = accumulated_for_stream.clone();
        async move {
            match item {
                Ok(delta) => {
                    accumulated.lock().await.push_str(&delta);
                    Ok(Event::default().data(delta))
                }
                Err(ProviderError::Request(e)) => {
                    Ok(Event::default().event("error").data(e.to_string()))
                }
                Err(ProviderError::Status(code, body)) => {
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{code}: {body}")))
                }
            }
        }
    });

    let chat_id_for_finish = chat_id.clone();
    let finishing_stream = sse_stream.chain(futures_util::stream::once(async move {
        let final_content = accumulated.lock().await.clone();
        if !final_content.is_empty() {
            if let Err(e) = writer
                .create_user_message_with_prompt(user_id, chat_id_for_finish,
                    Some(parent_id_for_reply),
                    final_content,
                    raw_prompt,
                    prompt_tokens,
                    context_limit,
                )
                .await
            {
                tracing::error!(error = %e, "failed to save user message");
            }
        }
        Ok(Event::default().event("done").data(""))
    }));

    Ok(Sse::new(finishing_stream).keep_alive(KeepAlive::default()))
}

#[derive(Deserialize)]
pub struct GenerateFieldInput {
    pub field: String,
    pub trait_name: Option<String>,
}

pub async fn generate_character_field(Extension(user_id): Extension<i64>, 
    State(state): State<AppState>,
    Path(character_id): Path<String>,
    axum::Json(input): axum::Json<GenerateFieldInput>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, crate::error::ApiError> {
    state.check_generation_rate_limit(user_id).await?;
    let character = crate::models::character::get(&state.db.read_pool, user_id, &character_id)
        .await?
        .ok_or_else(|| crate::error::ApiError::from(StatusCode::NOT_FOUND))?;

    let mut instruction;
    let mut post;

    match input.field.as_str() {
        "scenario" => {
            instruction = "Detailed description of the scene that the character is in".to_string();
            post = "Scenario:".to_string();
        }
        "appearance" => {
            instruction = "Very brief and comma-separated list of BOORU TAGS of the character's gender, eye color, hair color, height, clothes, body, physical location and surroundings".to_string();
            post = "Booru Tags:".to_string();
        }
        "persona" => {
            if let Some(ref t) = input.trait_name {
                instruction = format!("Provide a description of {}'s \"{}\" personality trait", character.name, t);
                post = format!("{}:", t);
            } else {
                instruction = format!("Provide an outline of the personality and typical behavior of {}", character.name);
                post = "Personality:".to_string();
            }
        }
        "greeting" => {
            instruction = format!("Provide {}'s first opening dialogue and actions in the scene", character.name);
            post = format!("{}'s Greeting:", character.name);
        }
        "sampleChat" => {
            instruction = format!("Provide an example of {}'s dialogue and actions", character.name);
            post = "Response:".to_string(); // old aetheria uses response: as default fallback
        }
        _ => return Err(StatusCode::BAD_REQUEST.into()),
    }

    let mut system_prompt = "You are a character generator. Provide information and attributes about the following character.\n\n".to_string();
    system_prompt.push_str(&format!("<instruct>Character's name:</instruct>\n{}\n\n", character.name));
    if !character.description.is_empty() {
        system_prompt.push_str(&format!("<instruct>Character's description:</instruct>\n{}\n\n", character.description));
    }

    // append infix data
    for field in &["appearance", "scenario", "persona", "greeting", "sampleChat"] {
        if *field == input.field.as_str() {
            continue;
        }
        match *field {
            "scenario" if !character.scenario.is_empty() => {
                system_prompt.push_str(&format!("<instruct>Detailed description of the scene that the character is in</instruct>\n{}\n\n", character.scenario));
            }
            "greeting" if !character.first_message.is_empty() => {
                system_prompt.push_str(&format!("<instruct>Provide {}'s first opening dialogue and actions in the scene</instruct>\n{}\n\n", character.name, character.first_message));
            }
            "sampleChat" if !character.sample_chat.is_empty() => {
                system_prompt.push_str(&format!("<instruct>Provide an example of {}'s dialogue and actions</instruct>\n{}\n\n", character.name, character.sample_chat));
            }
            "persona" => {
                // simple fallback if persona exists
                if !character.personality.is_empty() {
                    system_prompt.push_str(&format!("<instruct>Provide an outline of the personality and typical behavior of {}</instruct>\n{}\n\n", character.name, character.personality));
                }
            }
            _ => {}
        }
    }

    system_prompt.push_str(&format!("<instruct>{}</instruct>\n\n{}", instruction, post));

    let messages = vec![
        crate::provider::prompt::ChatMessage {
            role: crate::provider::prompt::Role::System,
            content: "You are a character generator. Follow the user's instructions exactly and only output the requested field.".to_string(),
        },
        crate::provider::prompt::ChatMessage {
            role: crate::provider::prompt::Role::User,
            content: system_prompt,
        }
    ];

    let settings = crate::models::settings::get_view(&state.db.read_pool, user_id).await?;
    let api_key = crate::models::settings::get_decrypted_api_key(&state.db.read_pool, user_id, &state.encryption_key).await?;

    let provider = resolve_provider(&settings.provider_type)?;

    let sampling = settings.sampling_params();
    let provider_stream = provider.stream_completion(state.http_client.clone(), settings.api_base_url, api_key, settings.model_name, messages, sampling).await;

    let sse_stream = provider_stream.map(|item| match item {
        Ok(delta) => Ok(Event::default().data(delta)),
        Err(ProviderError::Request(e)) => Ok(Event::default().event("error").data(e.to_string())),
        Err(ProviderError::Status(code, body)) => Ok(Event::default().event("error").data(format!("{code}: {body}"))),
    });

    let finishing_stream = sse_stream.chain(futures_util::stream::once(async move {
        Ok(Event::default().event("done").data(""))
    }));

    Ok(Sse::new(finishing_stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod resolve_provider_tests {
    use super::resolve_provider;

    #[test]
    fn recognized_provider_types_resolve() {
        for name in ["anthropic", "gemini", "horde", "novelai", "openai", "kobold", "mancer"] {
            assert!(resolve_provider(name).is_ok(), "'{name}' should resolve to a provider");
        }
    }

    #[test]
    fn unrecognized_provider_type_fails_loudly_instead_of_defaulting_to_openai() {
        match resolve_provider("totally-not-a-provider") {
            Err(e) => assert_eq!(e.code, 422),
            Ok(_) => panic!("expected an unrecognized provider_type to be rejected, not silently resolved"),
        }
    }
}
