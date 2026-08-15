use leptos::prelude::*;
use leptos::task::spawn_local;

use super::chat_state::{ChatSignals, is_scrolled_to_bottom, scroll_to_bottom_soon};

pub fn build_delete(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + Copy + 'static,
) -> impl Fn(String) + Clone + Copy + 'static {
    move |message_id: String| {
        spawn_local(async move {
            match crate::api::delete_message(&message_id).await {
                Ok(()) => {
                    signals.set_error.set(None);
                    fetch_tree();
                }
                Err(e) => signals.set_error.set(Some(e)),
            }
        });
    }
}

pub fn build_delete_all_below(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + Copy + 'static,
) -> impl Fn(String) + Clone + Copy + 'static {
    move |message_id: String| {
        let branch = signals.active_branch.get_untracked();
        if let Some(idx) = branch.iter().position(|m| m.id == message_id) {
            let to_delete: Vec<String> = branch[idx..].iter().map(|m| m.id.clone()).collect();
            spawn_local(async move {
                for id in to_delete.into_iter().rev() {
                    if let Err(e) = crate::api::delete_message(&id).await {
                        signals.set_error.set(Some(e));
                        fetch_tree();
                        return;
                    }
                }
                signals.set_error.set(None);
                fetch_tree();
            });
        }
    }
}

pub fn build_toggle_visibility(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + Copy + 'static,
) -> impl Fn(String, bool) + Clone + Copy + 'static {
    move |message_id: String, visible: bool| {
        spawn_local(async move {
            if let Err(e) = crate::api::set_message_visibility(&message_id, !visible).await {
                signals.set_error.set(Some(e));
            } else {
                fetch_tree();
            }
        });
    }
}

pub fn build_save_edit(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + Copy + 'static,
) -> impl Fn(String) + Clone + Copy + 'static {
    move |message_id: String| {
        let content = signals.edit_text.get_untracked();
        spawn_local(async move {
            if let Err(e) = crate::api::edit_message(&message_id, &content).await {
                signals.set_error.set(Some(e));
            } else {
                signals.set_editing_id.set(None);
                fetch_tree();
            }
        });
    }
}

pub fn build_select_sibling(signals: ChatSignals) -> impl Fn(String, String) + Clone + Copy + 'static {
    move |parent_id: String, child_id: String| {
        let cid = signals.chat_id.get_untracked();
        let was_at_bottom = is_scrolled_to_bottom();

        signals.set_selected_children.update(|map| {
            map.insert(parent_id.clone(), child_id.clone());
        });

        // a sibling can be a different length than the one it replaced;
        // if the view was pinned to the bottom before switching, keep it
        // pinned instead of leaving the scroll position stuck wherever it
        // happened to land relative to the top of the new content
        if was_at_bottom {
            scroll_to_bottom_soon();
        }

        spawn_local(async move {
            if let Ok(extra) = crate::api::subtree(&cid, &child_id, 50).await {
                signals.set_tree.update(|t| {
                    for (id, node) in extra {
                        if !t.messages.contains_key(&id) {
                            if let Some(pid) = &node.parent_id {
                                if let Some(parent) = t.messages.get_mut(pid) {
                                    if !parent.children.contains(&id) {
                                        parent.children.push(id.clone());
                                    }
                                }
                            }
                            t.messages.insert(id, node);
                        }
                    }
                });
            }
        });
    }
}

pub fn build_select_branch_point(signals: ChatSignals) -> impl Fn(String) + Clone + Copy + 'static {
    move |message_id: String| {
        let current = signals.send_parent_id.get();
        let new_val = if current.as_ref() == Some(&message_id) {
            None
        } else {
            Some(message_id)
        };
        signals.set_send_parent_id.set(new_val);
    }
}
