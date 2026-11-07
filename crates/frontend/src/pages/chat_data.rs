use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;

use super::chat_state::{is_scrolled_to_bottom, scroll_to_bottom_soon, ChatSignals};

pub struct LoadedChat {
    pub tree: crate::api::MessageTree,
    pub chat: Option<crate::api::Chat>,
    /// built from the active branch alone because the full tree would not
    /// load. shows the chat, but not the other branches
    pub partial: bool,
}

/// loads the chat's message tree without touching the signals. a failed load
/// is `None` so the caller can keep whatever is already on screen instead of
/// swapping in an empty tree. the branch fallback is only for an empty
/// screen. using it after a reply replaces the transcript with a shorter
/// chain, which is the wipe.
pub async fn load_tree(id: &str, allow_fallback: bool) -> Option<LoadedChat> {
    let (tree_data, chat) = futures_util::join!(async {
        let mut tree_data = None;
        for attempt in 0..2 {
            if attempt > 0 {
                gloo_timers::future::TimeoutFuture::new(350).await;
            }
            if let Ok(t) = crate::api::list_messages(id).await {
                tree_data = Some(t);
                break;
            }
        }
        tree_data
    }, crate::api::get_chat(id));
    let mut partial = false;
    let mut tree_data = match tree_data {
        Some(t) => t,
        None if allow_fallback => {
            partial = true;
            let branch_data = crate::api::active_branch(id).await.ok()?;
            let mut t = crate::api::MessageTree::default();
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
            // the server keeps the newest messages of a long chat, so the
            // first one may not be the real root
            if root_id.is_none() {
                root_id = branch_data.first().map(|m| m.id.clone());
            }
            t.root_id = root_id;
            t.messages = msgs;
            t
        }
        None => return None,
    };

    let chat = match chat {
        Ok(chat) => {
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
            Some(chat)
        }
        Err(_) => None,
    };

    Some(LoadedChat { tree: tree_data, chat, partial })
}

/// (re)loads the active chat's message tree from the server, along with the
/// chat/character/group metadata needed to render it. reused after every
/// mutation (send, regenerate, edit, delete, ...) rather than each of those
/// patching the tree locally, since the tree's shape can change server-side
/// in ways a local patch would miss (e.g. summarization pruning branches).
/// a landed tree also drops a leftover streamed tail, unless a reply is
/// still being generated.
pub fn build_fetch_tree(signals: ChatSignals) -> impl Fn() + Clone + Copy + 'static {
    move || {
        let id = signals.chat_id.get();
        let was_at_bottom = is_scrolled_to_bottom();
        let allow_fallback = signals.tree.with_untracked(|t| t.messages.is_empty());
        spawn_local(async move {
            // the path to the server drops a request now and then. a chat that
            // fails to load is asked for again, and one that only loaded as a
            // single branch is swapped for the whole thing once that arrives,
            // instead of leaving a blank or half-empty screen until a reload
            const LATER_TRIES: u32 = 8;
            let mut we_set_error = false;
            let mut showing_something = false;
            let mut loaded = load_tree(&id, allow_fallback).await;
            let mut tries = 0;
            loop {
                // the page is one component reused across chats. drop a load
                // that finished after the user already moved on.
                if signals.chat_id.get_untracked() != id {
                    return;
                }
                let mut partial = false;
                if let Some(loaded) = loaded.take() {
                    partial = loaded.partial;
                    // a reply still streaming owns the tail. once it isn't, a
                    // landed tree replaces any text we were holding after a
                    // failed reload.
                    let still_streaming = signals.gen_for(&id).is_generating;
                    if let Some(chat) = loaded.chat {
                        signals.set_chat_meta.set(Some(chat));
                    }
                    signals.apply_tree(loaded.tree);
                    if !still_streaming {
                        signals.clear_live_tail(&id);
                    }
                    if was_at_bottom && !showing_something {
                        scroll_to_bottom_soon();
                    }
                    showing_something = true;
                    if we_set_error {
                        signals.set_error.set(None);
                        we_set_error = false;
                    }
                    if !partial {
                        return;
                    }
                } else if !showing_something && !we_set_error {
                    signals.set_error.set(Some("couldn't load this chat yet, still trying".to_string()));
                    we_set_error = true;
                }
                tries += 1;
                if tries > LATER_TRIES {
                    if !showing_something && we_set_error {
                        signals.set_error.set(Some("couldn't load this chat, reload the page to try again".to_string()));
                    }
                    return;
                }
                gloo_timers::future::TimeoutFuture::new(3_000).await;
                // later tries want the whole tree, never the shorter fallback
                loaded = load_tree(&id, false).await;
            }
        });
    }
}
