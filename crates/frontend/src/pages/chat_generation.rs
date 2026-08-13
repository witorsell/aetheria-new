use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;

use super::chat_helpers::regeneration_parent_id;
use super::chat_state::{ChatSignals, is_near_bottom, scroll_to_bottom_soon};

/// the streaming plumbing shared by every generation kind (send, regenerate,
/// continue, respond-as-me): push deltas into `streaming_reply`, keep the
/// view scrolled to the bottom while it grows, and route a session-expiry
/// error to the login page instead of showing it as a generic stream error.
/// `track_member_switches` is false for continue/respond-as-me, which never
/// hand off between group members mid-stream.
async fn run_stream_generation(
    url: String,
    body: Option<String>,
    signals: ChatSignals,
    track_member_switches: bool,
    navigate: impl Fn(&str, NavigateOptions) + 'static,
) {
    let result = crate::api::stream_post(
        &url,
        body,
        move |delta| {
            let should_scroll = is_near_bottom();
            signals.set_streaming_reply.update(|current| current.push_str(&delta));
            if should_scroll {
                scroll_to_bottom_soon();
            }
        },
        move |character_id: String, name: String| {
            if !track_member_switches {
                return;
            }
            let previous = signals.current_member.get_untracked();
            if let Some((prev_id, prev_name)) = previous {
                let content = signals.streaming_reply.get_untracked();
                if !content.is_empty() {
                    signals.set_group_reply_log.update(|log| log.push((prev_id, prev_name, content)));
                }
            }
            signals.set_streaming_reply.set(String::new());
            signals.set_current_member.set(Some((character_id, name)));
        },
        move |err| {
            if err == crate::api::SESSION_EXPIRED_ERROR {
                navigate("/login", NavigateOptions::default());
            } else {
                signals.set_stream_error.set(true);
                signals.set_error.set(Some(err));
            }
        },
    )
    .await;
    if let Err(e) = result {
        signals.set_stream_error.set(true);
        signals.set_error.set(Some(e));
    }
}

pub fn build_send(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + 'static,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::SubmitEvent) + 'static {
    move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let content = signals.draft.get_untracked();
        if content.is_empty() || signals.is_generating.get_untracked() {
            return;
        }
        signals.set_draft.set(String::new());
        signals.set_error.set(None);
        signals.set_stream_error.set(false);
        signals.set_is_generating.set(true);
        signals.set_is_self_reply.set(false);
        signals.set_streaming_reply.set(String::new());
        signals.set_current_member.set(None);
        signals.set_group_reply_log.set(Vec::new());
        signals.set_pending_user_text.set(Some(content.clone()));

        scroll_to_bottom_soon();

        let id = signals.chat_id.get_untracked();
        // branch point was picked, otherwise every send with no parent
        // starts a brand new root and orphans everything before it.
        let parent = signals
            .send_parent_id
            .get_untracked()
            .or_else(|| signals.active_branch.get_untracked().last().map(|m| m.id.clone()));
        let body = serde_json::json!({ "content": content, "parent_id": parent }).to_string();
        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/generate");
            run_stream_generation(url, Some(body), signals, true, navigate).await;
            signals.set_is_generating.set(false);
            signals.set_streaming_reply.set(String::new());
            signals.set_current_member.set(None);
            signals.set_group_reply_log.set(Vec::new());
            signals.set_pending_user_text.set(None);
            signals.set_send_parent_id.set(None);
            signals.set_regenerating_msg_id.set(None);
            // the new message is the sole child of whatever branch was
            // active, so it's already what walk_active_branch defaults to
            // there - clearing the whole map used to also wipe every
            // earlier fork's selection (e.g. original vs. regenerated),
            // silently rerouting the view through the newest sibling at
            // each one instead of the branch actually being replied to
            fetch_tree();
        });
    }
}

pub fn build_regenerate(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + 'static,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn() + Clone + 'static {
    move || {
        if signals.is_generating.get_untracked() {
            return;
        }
        signals.set_error.set(None);
        signals.set_stream_error.set(false);
        signals.set_is_generating.set(true);
        signals.set_is_self_reply.set(false);
        signals.set_streaming_reply.set(String::new());
        signals.set_current_member.set(None);
        signals.set_group_reply_log.set(Vec::new());

        let id = signals.chat_id.get_untracked();
        let branch = signals.active_branch.get_untracked();
        let mut regen_character_id = None;
        if let Some(last) = branch.last() {
            if last.role == "assistant" {
                signals.set_regenerating_msg_id.set(Some(last.id.clone()));
                regen_character_id = last.character_id.clone();
            }
        }
        // a group reroll targets one named member (the one whose turn
        // is being redone), so the backend needs character_id on top of
        // the usual parent_id branch point. without it a group chat's
        // regenerate call just 400s. messages written before run_group_generation
        // ever touched them (greetings, or replies from back when the chat
        // was still 1:1) have no character_id of their own, so fall back to
        // the group's first enabled member rather than silently dropping the param
        let group = signals.tree.get_untracked().group;
        let is_group_chat = group.is_some();
        if regen_character_id.is_none() {
            regen_character_id = group.as_ref().and_then(|g| {
                g.members.iter().find(|m| !m.disabled).map(|m| m.character_id.clone())
            });
        }
        let mut query_parts = Vec::new();
        if let Some(pid) = regeneration_parent_id(&branch) {
            query_parts.push(format!("parent_id={pid}"));
        }
        if is_group_chat {
            if let Some(cid) = regen_character_id {
                query_parts.push(format!("character_id={cid}"));
            }
        }
        let parent_param = if query_parts.is_empty() {
            String::new()
        } else {
            format!("?{}", query_parts.join("&"))
        };
        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/regenerate{parent_param}");
            run_stream_generation(url, None, signals, true, navigate).await;
            signals.set_is_generating.set(false);
            signals.set_streaming_reply.set(String::new());
            signals.set_current_member.set(None);
            signals.set_group_reply_log.set(Vec::new());
            signals.set_regenerating_msg_id.set(None);
            fetch_tree();
        });
    }
}

pub fn build_continue_gen(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + 'static,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::MouseEvent) + Clone + 'static {
    move |_| {
        if signals.is_generating.get_untracked() {
            return;
        }
        signals.set_error.set(None);
        signals.set_stream_error.set(false);
        signals.set_is_generating.set(true);
        signals.set_is_self_reply.set(false);
        signals.set_more_menu_open.set(false);
        signals.set_streaming_reply.set(String::new());
        signals.set_current_member.set(None);
        signals.set_group_reply_log.set(Vec::new());

        let id = signals.chat_id.get_untracked();
        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/continue");
            run_stream_generation(url, None, signals, false, navigate).await;
            signals.set_is_generating.set(false);
            signals.set_streaming_reply.set(String::new());
            signals.set_current_member.set(None);
            signals.set_group_reply_log.set(Vec::new());
            fetch_tree();
        });
    }
}

pub fn build_respond_as_me(
    signals: ChatSignals,
    fetch_tree: impl Fn() + Clone + 'static,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::MouseEvent) + Clone + 'static {
    move |_| {
        if signals.is_generating.get_untracked() {
            return;
        }
        signals.set_error.set(None);
        signals.set_stream_error.set(false);
        signals.set_is_generating.set(true);
        signals.set_is_self_reply.set(true);
        signals.set_more_menu_open.set(false);
        signals.set_streaming_reply.set(String::new());
        signals.set_current_member.set(None);
        signals.set_group_reply_log.set(Vec::new());

        let id = signals.chat_id.get_untracked();
        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/respond-as-user");
            run_stream_generation(url, None, signals, false, navigate).await;
            signals.set_is_generating.set(false);
            signals.set_is_self_reply.set(false);
            signals.set_streaming_reply.set(String::new());
            signals.set_current_member.set(None);
            signals.set_group_reply_log.set(Vec::new());
            fetch_tree();
        });
    }
}
