use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;

use super::chat_state::{is_scrolled_to_bottom, scroll_to_bottom_soon};

/// (re)loads the active chat's message tree from the server, along with the
/// chat/character/group metadata needed to render it. reused after every
/// mutation (send, regenerate, edit, delete, ...) rather than each of those
/// patching the tree locally, since the tree's shape can change server-side
/// in ways a local patch would miss (e.g. summarization pruning branches).
/// takes its three fields directly rather than the full `ChatSignals` bundle
/// so it can be built (and its mount-time Effect registered) before the rest
/// of the chat page's state exists.
pub fn build_fetch_tree(
    chat_id: Memo<String>,
    set_chat_meta: WriteSignal<Option<crate::api::Chat>>,
    set_tree: WriteSignal<crate::api::MessageTree>,
) -> impl Fn() + Clone + Copy + 'static {
    move || {
        let id = chat_id.get();
        spawn_local(async move {
            let was_at_bottom = is_scrolled_to_bottom();

            let mut tree_data = match crate::api::list_messages(&id).await {
                Ok(t) => t,
                Err(_) => {
                    let mut t = crate::api::MessageTree::default();
                    if let Ok(branch_data) = crate::api::active_branch(&id).await {
                        let mut msgs: HashMap<String, crate::api::MessageNode> = HashMap::new();
                        let mut root_id = None;
                        let mut prev_id: Option<String> = None;
                        for msg in &branch_data {
                            if msg.parent_id.is_none() {
                                root_id = Some(msg.id.clone());
                            }
                            if let Some(pid) = &prev_id {
                                if let Some(parent) = msgs.get_mut(pid) {
                                    if !parent.children.contains(&msg.id) {
                                        parent.children.push(msg.id.clone());
                                    }
                                }
                            }
                            msgs.insert(msg.id.clone(), msg.clone());
                            prev_id = Some(msg.id.clone());
                        }
                        t.root_id = root_id;
                        t.messages = msgs;
                    }
                    t
                }
            };

            if let Ok(chat) = crate::api::get_chat(&id).await {
                set_chat_meta.set(Some(chat.clone()));

                if tree_data.character.is_none() {
                    if let Some(cid) = &chat.character_id {
                        if let Ok(c) = crate::api::get_character(cid).await {
                            tree_data.character = Some(c);
                        }
                    }
                }
                if tree_data.group.is_none() {
                    if let Some(gid) = &chat.group_id {
                        if let Ok(gwm) = crate::api::get_group(gid).await {
                            tree_data.group = Some(gwm);
                        }
                    }
                }
            }

            set_tree.set(tree_data);

            if was_at_bottom {
                scroll_to_bottom_soon();
            }
        });
    }
}
