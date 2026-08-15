use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;

use super::chat_helpers::regeneration_parent_id;
use super::chat_state::{ChatSignals, is_near_bottom, scroll_to_bottom_soon};

/// the streaming plumbing shared by every generation kind (send, regenerate,
/// continue, respond-as-me): push deltas into `id`'s `GenState`, keep the
/// view scrolled to the bottom while it grows (only while `id` is still the
/// chat on screen), and route a session-expiry error to the login page
/// instead of showing it as a generic stream error. `id` is the chat this
/// stream belongs to, captured once by the caller before the request starts
/// - the user may navigate to a different chat while this is in flight, so
/// every write here targets `id`'s slot specifically rather than whatever
/// chat happens to be showing when a delta arrives.
/// `track_member_switches` is false for continue/respond-as-me, which never
/// hand off between group members mid-stream.
async fn run_stream_generation(
    url: String,
    body: Option<String>,
    signals: ChatSignals,
    id: String,
    track_member_switches: bool,
    navigate: impl Fn(&str, NavigateOptions) + 'static,
) {
    let result = crate::api::stream_post(
        &url,
        body,
        {
            let id = id.clone();
            move |delta| {
                let should_scroll = signals.chat_id.get_untracked() == id && is_near_bottom();
                signals.update_gen(&id, |s| s.streaming_reply.push_str(&delta));
                if should_scroll {
                    scroll_to_bottom_soon();
                }
            }
        },
        {
            let id = id.clone();
            move |character_id: String, name: String| {
                if !track_member_switches {
                    return;
                }
                signals.update_gen(&id, |s| {
                    if let Some((prev_id, prev_name)) = s.current_member.clone() {
                        if !s.streaming_reply.is_empty() {
                            s.group_reply_log.push((prev_id, prev_name, s.streaming_reply.clone()));
                        }
                    }
                    s.streaming_reply.clear();
                    s.current_member = Some((character_id, name));
                });
            }
        },
        {
            let id = id.clone();
            move |err| {
                if err == crate::api::SESSION_EXPIRED_ERROR {
                    navigate("/login", NavigateOptions::default());
                } else {
                    signals.update_gen(&id, |s| s.stream_error = true);
                    signals.set_error.set(Some(err));
                }
            }
        },
    )
    .await;
    if let Err(e) = result {
        signals.update_gen(&id, |s| s.stream_error = true);
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
        let id = signals.chat_id.get_untracked();
        if content.is_empty() || signals.gen_for(&id).is_generating {
            return;
        }
        signals.set_draft.set(String::new());
        signals.set_error.set(None);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.is_self_reply = false;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });
        signals.set_pending_user_text.set(Some(content.clone()));

        scroll_to_bottom_soon();

        // branch point was picked, otherwise every send with no parent
        // starts a brand new root and orphans everything before it.
        let parent = signals
            .send_parent_id
            .get_untracked()
            .or_else(|| signals.active_branch.get_untracked().last().map(|m| m.id.clone()));
        let body = serde_json::json!({ "content": content, "parent_id": parent }).to_string();
        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/generate");
            run_stream_generation(url, Some(body), signals, stream_id.clone(), true, navigate).await;
            signals.update_gen(&stream_id, |s| {
                s.is_generating = false;
                s.streaming_reply.clear();
                s.current_member = None;
                s.group_reply_log.clear();
                s.regenerating_msg_id = None;
            });
            signals.set_pending_user_text.set(None);
            signals.set_send_parent_id.set(None);
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
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).is_generating {
            return;
        }
        signals.set_error.set(None);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.is_self_reply = false;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });

        let branch = signals.active_branch.get_untracked();
        let mut regen_character_id = None;
        if let Some(last) = branch.last() {
            if last.role == "assistant" {
                signals.update_gen(&id, |s| s.regenerating_msg_id = Some(last.id.clone()));
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
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/regenerate{parent_param}");
            run_stream_generation(url, None, signals, stream_id.clone(), true, navigate).await;
            signals.update_gen(&stream_id, |s| {
                s.is_generating = false;
                s.streaming_reply.clear();
                s.current_member = None;
                s.group_reply_log.clear();
                s.regenerating_msg_id = None;
            });
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
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).is_generating {
            return;
        }
        signals.set_error.set(None);
        signals.set_more_menu_open.set(false);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.is_self_reply = false;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });

        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/continue");
            run_stream_generation(url, None, signals, stream_id.clone(), false, navigate).await;
            signals.update_gen(&stream_id, |s| {
                s.is_generating = false;
                s.streaming_reply.clear();
                s.current_member = None;
                s.group_reply_log.clear();
            });
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
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).is_generating {
            return;
        }
        signals.set_error.set(None);
        signals.set_more_menu_open.set(false);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.is_self_reply = true;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });

        let navigate = navigate.clone();
        let fetch_tree = fetch_tree.clone();
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/respond-as-user");
            run_stream_generation(url, None, signals, stream_id.clone(), false, navigate).await;
            signals.update_gen(&stream_id, |s| {
                s.is_generating = false;
                s.is_self_reply = false;
                s.streaming_reply.clear();
                s.current_member = None;
                s.group_reply_log.clear();
            });
            fetch_tree();
        });
    }
}
