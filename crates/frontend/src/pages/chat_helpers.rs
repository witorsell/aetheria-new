use leptos::prelude::*;
use std::collections::HashMap;

#[component]
pub(super) fn Thought(
    #[prop(into)] text: Signal<String>,
    #[prop(into)] message_id: String,
    #[prop(into)] open_states: Signal<HashMap<String, bool>>,
    #[prop(into)] set_open_states: WriteSignal<HashMap<String, bool>>,
    forbid_media: bool,
) -> impl IntoView {
    let tid: ReadSignal<String> = signal(message_id).0;
    let should_show = move || !text.get().is_empty();
    let is_open = move || open_states.get().get(&tid.get()).copied().unwrap_or(false);

    view! {
        <div>
            {move || should_show().then(move || {
                view! {
                    <div class="thought-toggle" on:click=move |_| {
                        let t = tid.get();
                        set_open_states.update(|map| {
                            let cur = map.get(&t).copied().unwrap_or(false);
                            map.insert(t, !cur);
                        });
                    }>
                        "Thought "{move || if is_open() { "-" } else { "+" }}
                    </div>
                }
            })}
            {move || is_open().then(move || {
                view! {
                    <div class="thought-content">
                        {move || crate::render::markdown::render_markdown(&text.get(), "", "", forbid_media)}
                    </div>
                }
            })}
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
/// how each subtree ranks when no swipe picked a child: one holding a reply
/// beats one that is only an unanswered line, then the newest message wins.
/// without the first part, a stray line that is newer than what is left of
/// the real branch (after its last messages were deleted) takes over the view.
fn newest_in_subtree(tree: &crate::api::MessageTree) -> HashMap<String, (bool, i64)> {
    let mut rows: Vec<&crate::api::MessageNode> = tree.messages.values().collect();
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut newest: HashMap<String, i64> = HashMap::new();
    let mut answered: std::collections::HashSet<String> = std::collections::HashSet::new();
    for node in rows {
        let mut cur = Some(node.id.as_str());
        while let Some(id) = cur {
            if newest.contains_key(id) {
                break;
            }
            newest.insert(id.to_string(), node.created_at);
            cur = tree.messages.get(id).and_then(|n| n.parent_id.as_deref());
        }
        if node.role == "assistant" {
            let mut cur = Some(node.id.as_str());
            while let Some(id) = cur {
                if !answered.insert(id.to_string()) {
                    break;
                }
                cur = tree.messages.get(id).and_then(|n| n.parent_id.as_deref());
            }
        }
    }
    newest
        .into_iter()
        .map(|(id, t)| {
            let a = answered.contains(&id);
            (id, (a, t))
        })
        .collect()
}

pub(super) fn walk_active_branch(
    tree: &crate::api::MessageTree,
    selected_children: &HashMap<String, String>,
) -> Vec<crate::api::MessageNode> {
    let newest = newest_in_subtree(tree);
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
        // a swipe that points at a message which was deleted is not a leaf.
        // stopping here is what left only the opening exchange on screen.
        let chosen = selected_children.get(&node.id).filter(|id| tree.messages.contains_key(id.as_str()));
        current_id = chosen
            .cloned()
            .or_else(|| {
                // no swipe picked: follow the child whose subtree holds the
                // newest message, so a dead-end sibling never hides the chat
                node.children
                    .iter()
                    .filter(|id| tree.messages.contains_key(id.as_str()))
                    .max_by_key(|id| newest.get(id.as_str()).copied().unwrap_or((false, 0)))
                    .cloned()
            })
            .unwrap_or_default();
        if current_id.is_empty() {
            break;
        }
    }

    branch
}

#[cfg(test)]
mod tests {
    use super::walk_active_branch;
    use std::collections::HashMap;

    fn node(id: &str, parent: Option<&str>, children: &[&str]) -> crate::api::MessageNode {
        crate::api::MessageNode {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            role: "assistant".to_string(),
            content: String::new(),
            visible: true,
            deleted: false,
            created_at: 0,
            children: children.iter().map(|s| s.to_string()).collect(),
            raw_prompt: None,
            prompt_tokens: None,
            context_limit: None,
            character_id: None,
        }
    }

    fn tree(root: &str, nodes: Vec<crate::api::MessageNode>) -> crate::api::MessageTree {
        crate::api::MessageTree {
            root_id: Some(root.to_string()),
            messages: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            character: None,
            group: None,
        }
    }

    #[test]
    fn a_missing_swipe_does_not_stop_the_walk() {
        let tree = tree(
            "a",
            vec![
                node("a", None, &["b"]),
                node("b", Some("a"), &["c", "d"]),
                node("c", Some("b"), &[]),
                node("d", Some("b"), &[]),
            ],
        );
        let mut selected = HashMap::new();
        selected.insert("b".to_string(), "gone".to_string());
        let ids: Vec<_> = walk_active_branch(&tree, &selected).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["a", "b", "d"]);
    }

    #[test]
    fn a_newer_dead_end_sibling_does_not_hide_the_long_branch() {
        let mut stray = node("stray", Some("a"), &[]);
        stray.created_at = 6;
        let mut long = node("long", Some("a"), &["l2"]);
        long.created_at = 2;
        let mut l2 = node("l2", Some("long"), &[]);
        l2.created_at = 9;
        let mut root = node("a", None, &["long", "stray"]);
        root.created_at = 1;
        let tree = tree("a", vec![root, long, l2, stray]);
        let ids: Vec<_> = walk_active_branch(&tree, &HashMap::new()).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["a", "long", "l2"]);
    }

    #[test]
    fn an_unanswered_line_newer_than_the_branch_tail_stays_hidden() {
        let mut root = node("a", None, &["long", "stray"]);
        root.created_at = 1;
        let mut long = node("long", Some("a"), &[]);
        long.created_at = 2;
        let mut stray = node("stray", Some("a"), &[]);
        stray.created_at = 8;
        stray.role = "user".to_string();
        let tree = tree("a", vec![root, long, stray]);
        let ids: Vec<_> = walk_active_branch(&tree, &HashMap::new()).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["a", "long"]);
    }

    #[test]
    fn a_live_swipe_is_still_followed() {
        let tree = tree(
            "a",
            vec![
                node("a", None, &["b"]),
                node("b", Some("a"), &["c", "d"]),
                node("c", Some("b"), &[]),
                node("d", Some("b"), &[]),
            ],
        );
        let mut selected = HashMap::new();
        selected.insert("b".to_string(), "c".to_string());
        let ids: Vec<_> = walk_active_branch(&tree, &selected).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }
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
    character_by_id: &HashMap<String, crate::api::CharacterSummary>,
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
