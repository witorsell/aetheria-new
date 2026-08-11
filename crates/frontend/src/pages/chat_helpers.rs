use leptos::prelude::*;
use std::collections::HashMap;

#[component]
pub(super) fn Thought(#[prop(into)] text: Signal<String>, forbid_media: bool) -> impl IntoView {
    let (open, set_open) = signal(false);

    view! {
        <div>
            <Show when=move || !text.get().is_empty()>
                <div class="thought-toggle" on:click=move |_| set_open.update(|o| *o = !*o)>
                    "Thought "{move || if open.get() { "-" } else { "+" }}
                </div>
            </Show>
            <Show when=move || open.get()>
                <div class="thought-content">
                    {move || crate::render::markdown::render_markdown(&text.get(), "", "", forbid_media)}
                </div>
            </Show>
        </div>
    }
}

#[component]
pub(super) fn RawPromptPanel(
    raw_prompt: String,
    prompt_tokens: Option<i64>,
    context_limit: Option<i64>,
) -> impl IntoView {
    let parsed = crate::api::parse_raw_prompt(&raw_prompt);
    let used = prompt_tokens.unwrap_or(0);
    let limit = context_limit.unwrap_or(0);
    let usage_text = if limit > 0 {
        format!("{used} / {limit} tokens (estimated)")
    } else {
        format!("{used} tokens (estimated, no limit set)")
    };

    view! {
        <div class="raw-prompt-panel">
            <div class="raw-prompt-usage">{usage_text}</div>
            <div class="raw-prompt-messages">
                {parsed
                    .into_iter()
                    .map(|m| {
                        view! {
                            <div class="raw-prompt-message">
                                <span class="raw-prompt-role">{m.role}</span>
                                <pre>{m.content}</pre>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// walk the active branch from root to leaf, following the selected child at
/// each node. if no selection is recorded for a node, follows the last child
/// (most recent). returns an ordered list of nodes.
pub(super) fn walk_active_branch(
    tree: &crate::api::MessageTree,
    selected_children: &HashMap<String, String>,
) -> Vec<crate::api::MessageNode> {
    let mut branch = Vec::new();
    let mut current_id = match &tree.root_id {
        Some(id) => id.clone(),
        None => return branch,
    };

    loop {
        let node = match tree.messages.get(&current_id) {
            Some(n) => n.clone(),
            None => break,
        };
        if node.children.is_empty() {
            branch.push(node);
            break;
        }
        branch.push(node.clone());
        current_id = selected_children
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| node.children.last().unwrap().clone());
    }

    branch
}

pub(super) fn regeneration_parent_id(branch: &[crate::api::MessageNode]) -> Option<String> {
    branch.last().and_then(|last| {
        if last.role == "assistant" {
            last.parent_id.clone()
        } else {
            None
        }
    })
}

/// figures out which character a given assistant message actually came from.
/// looks it up by character_id in the group member map first, falling back
/// to the chat's single fixed character for 1:1 chats (or if the id's missing).
pub(super) fn speaker_for(
    character_by_id: &HashMap<String, crate::api::Character>,
    fallback_character: &Option<crate::api::Character>,
    character_id: &Option<String>,
) -> (String, Option<String>) {
    if let Some(cid) = character_id {
        if let Some(c) = character_by_id.get(cid) {
            return (c.name.clone(), c.avatar_url.clone());
        }
    }
    (
        fallback_character.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "assistant".to_string()),
        fallback_character.as_ref().and_then(|c| c.avatar_url.clone()),
    )
}
