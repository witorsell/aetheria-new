use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;

use super::chat_helpers::regeneration_parent_id;
use super::chat_state::{
    is_scroll_at_top, is_scrolled_to_bottom, scroll_to_bottom_settled, scroll_to_bottom_soon, ChatSignals,
};

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
                let showing = signals.chat_id.get_untracked() == id;
                let mut follow = false;
                signals.update_gen(&id, |s| {
                    if showing {
                        let at_bottom = is_scrolled_to_bottom();
                        let at_top = is_scroll_at_top();
                        // parked in the middle: they left the tail. a clamp
                        // to scroll 0 still reports at_top, so it keeps following.
                        if !super::chat_state::keep_following(s.follow_tail, at_bottom, at_top) {
                            s.follow_tail = false;
                        }
                    }
                    follow = s.follow_tail;
                    s.streaming_reply.push_str(&delta);
                });
                if showing && follow {
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
                    signals.set_error.set(Some(crate::api::brief_net_error(err)));
                }
            }
        },
    )
    .await;
    if let Err(e) = result {
        signals.update_gen(&id, |s| s.stream_error = true);
        signals.set_error.set(Some(crate::api::brief_net_error(e)));
    }
}

fn release_tail_hold(signals: &ChatSignals, id: &str) {
    signals.update_gen(id, |s| s.tail_hold = false);
}

/// send the next queued line once nothing is in flight. `from_user` is an
/// explicit submit, which is allowed to release a queue left behind by a
/// stream error. an automatic pump after a failed reply leaves the lines
/// sitting until they hit send.
fn pump_queue(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
    from_user: bool,
) {
    let id = signals.chat_id.get_untracked();
    let state = signals.gen_for(&id);
    if state.blocks_send() {
        return;
    }
    if state.stream_error && !from_user {
        return;
    }
    let Some(content) = signals.pop_outbox(&id) else {
        return;
    };
    start_send(signals, navigate, id, content);
}

/// swap the streamed tail for the saved messages in one turn. the tail stays
/// up through the reload, including when the reload fails, so a finished
/// reply never blinks out of the history. a queued line sends only after
/// that saved row is on screen, and only for the chat still open.
async fn finish_stream(
    signals: ChatSignals,
    stream_id: String,
    clear_send_parent: bool,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) {
    if signals.chat_id.get_untracked() != stream_id {
        signals.clear_live_tail(&stream_id);
        release_tail_hold(&signals, &stream_id);
        return;
    }
    // stop the cursor and show the whole reply now. the history reload is a
    // separate request and used to hold the "still generating" state for
    // seconds after the last token was already on screen. the text stays
    // until that reload lands, so a failed one still doesn't blank it.
    // tail_hold keeps send from starting a second reply during that reload.
    signals.update_gen(&stream_id, |s| {
        s.is_generating = false;
        s.tail_hold = true;
    });
    let follow_now = signals.gen_for(&stream_id).follow_tail || is_scrolled_to_bottom();
    if follow_now {
        scroll_to_bottom_soon();
    }
    match super::chat_data::load_tree(&stream_id, false).await {
        Some(loaded) => {
            if signals.chat_id.get_untracked() != stream_id {
                signals.clear_live_tail(&stream_id);
                release_tail_hold(&signals, &stream_id);
                return;
            }
            if let Some(chat) = loaded.chat {
                signals.set_chat_meta.set(Some(chat));
            }
            // read before clear_live_tail. follow_tail survives a clamp to
            // the top, which is_scrolled_to_bottom() would miss, so the
            // reload doesn't leave them on the opening exchange.
            let follow = signals.gen_for(&stream_id).follow_tail || is_scrolled_to_bottom();
            signals.apply_tree(loaded.tree);
            signals.clear_live_tail(&stream_id);
            if clear_send_parent {
                signals.set_send_parent_id.set(None);
            }
            if follow {
                scroll_to_bottom_settled();
            }
            release_tail_hold(&signals, &stream_id);
            pump_queue(signals, navigate, false);
        }
        None => {
            // stop the cursor, keep the text. a failed reload must not
            // swap in a shorter branch. try once more in the background
            // so the saved rows can still replace the tail. the queue stays
            // held until that retry lands.
            let follow = signals.gen_for(&stream_id).follow_tail;
            signals.update_gen(&stream_id, |s| s.is_generating = false);
            if follow {
                scroll_to_bottom_settled();
            }
            let retry_id = stream_id.clone();
            let navigate = navigate.clone();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(1200).await;
                if signals.chat_id.get_untracked() != retry_id {
                    release_tail_hold(&signals, &retry_id);
                    return;
                }
                let Some(loaded) = super::chat_data::load_tree(&retry_id, false).await else {
                    release_tail_hold(&signals, &retry_id);
                    return;
                };
                if signals.chat_id.get_untracked() != retry_id {
                    release_tail_hold(&signals, &retry_id);
                    return;
                }
                if let Some(chat) = loaded.chat {
                    signals.set_chat_meta.set(Some(chat));
                }
                let follow = signals.gen_for(&retry_id).follow_tail || is_scrolled_to_bottom();
                signals.apply_tree(loaded.tree);
                signals.clear_live_tail(&retry_id);
                release_tail_hold(&signals, &retry_id);
                if follow {
                    scroll_to_bottom_settled();
                }
                pump_queue(signals, navigate, false);
            });
        }
    }
}

fn start_send(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
    id: String,
    content: String,
) {
    signals.set_error.set(None);
    // a reply can still be sitting in the tail because its reload failed.
    // clearing it and parenting the new line on the previous user turn
    // is how later messages became siblings and dropped off the branch.
    let had_unsaved = !signals.gen_for(&id).streaming_reply.is_empty();
    signals.update_gen(&id, |s| {
        s.stream_error = false;
        s.is_generating = true;
        s.tail_hold = false;
        s.follow_tail = true;
        s.is_self_reply = false;
        if !had_unsaved {
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        }
    });
    signals.set_pending_user_text.set(Some(content.clone()));

    scroll_to_bottom_soon();

    let navigate = navigate.clone();
    let stream_id = id;
    let content_for_send = content;
    spawn_local(async move {
        if had_unsaved {
            let settled = super::chat_data::load_tree(&stream_id, false).await;
            if signals.chat_id.get_untracked() != stream_id {
                signals.update_gen(&stream_id, |s| s.is_generating = false);
                signals.push_front_outbox(&stream_id, &content_for_send);
                signals.set_pending_user_text.set(None);
                return;
            }
            match settled {
                Some(loaded) => {
                    if let Some(chat) = loaded.chat {
                        signals.set_chat_meta.set(Some(chat));
                    }
                    signals.apply_tree(loaded.tree);
                    signals.clear_live_tail(&stream_id);
                    signals.set_pending_user_text.set(Some(content_for_send.clone()));
                    signals.update_gen(&stream_id, |s| {
                        s.is_generating = true;
                        s.tail_hold = false;
                        s.follow_tail = true;
                        s.stream_error = false;
                    });
                }
                None => {
                    signals.update_gen(&stream_id, |s| s.is_generating = false);
                    signals.set_pending_user_text.set(None);
                    if signals.chat_id.get_untracked() == stream_id && signals.draft.get_untracked().trim().is_empty() {
                        signals.set_draft.set(content_for_send);
                    } else {
                        signals.push_front_outbox(&stream_id, &content_for_send);
                    }
                    signals.set_error.set(Some("connection dropped".into()));
                    return;
                }
            }
        }
        // branch point was picked, otherwise every send with no parent
        // starts a brand new root and orphans everything before it.
        let parent = signals
            .send_parent_id
            .get_untracked()
            .or_else(|| signals.active_branch.get_untracked().last().map(|m| m.id.clone()));
        let body = serde_json::json!({ "content": content_for_send, "parent_id": parent }).to_string();
        let url = format!("/api/chats/{stream_id}/generate");
        run_stream_generation(url, Some(body), signals, stream_id.clone(), true, navigate.clone()).await;
        // the new message is the sole child of whatever branch was
        // active, so it's already what walk_active_branch defaults to
        // there - clearing the whole selection map used to also wipe
        // every earlier fork (e.g. original vs. regenerated) and
        // silently reroute the view through the newest sibling
        finish_stream(signals, stream_id, true, navigate).await;
    });
}

pub fn build_send(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::SubmitEvent) + 'static {
    move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let raw = signals.draft.get_untracked();
        let content = raw.trim().to_string();
        let id = signals.chat_id.get_untracked();
        let busy = signals.gen_for(&id).blocks_send();
        if content.is_empty() {
            // spaces are not a message. an empty send releases a queue that
            // was held after an error, or left behind by switching chats.
            if !raw.is_empty() {
                signals.set_draft.set(String::new());
            }
            if !busy {
                pump_queue(signals, navigate.clone(), true);
            }
            return;
        }
        if !signals.enqueue_outbox(&id, &content) {
            return;
        }
        signals.set_draft.set(String::new());
        if !busy {
            pump_queue(signals, navigate.clone(), true);
        }
    }
}

pub fn build_regenerate(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn() + Clone + 'static {
    move || {
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).blocks_send() {
            return;
        }
        signals.set_error.set(None);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.follow_tail = true;
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
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/regenerate{parent_param}");
            run_stream_generation(url, None, signals, stream_id.clone(), true, navigate.clone()).await;
            finish_stream(signals, stream_id, false, navigate).await;
        });
    }
}

pub fn build_continue_gen(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::MouseEvent) + Clone + 'static {
    move |_| {
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).blocks_send() {
            return;
        }
        signals.set_error.set(None);
        signals.set_more_menu_open.set(false);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.follow_tail = true;
            s.is_self_reply = false;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });

        let navigate = navigate.clone();
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/continue");
            run_stream_generation(url, None, signals, stream_id.clone(), false, navigate.clone()).await;
            finish_stream(signals, stream_id, false, navigate).await;
        });
    }
}

pub fn build_respond_as_me(
    signals: ChatSignals,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
) -> impl Fn(leptos::ev::MouseEvent) + Clone + 'static {
    move |_| {
        let id = signals.chat_id.get_untracked();
        if signals.gen_for(&id).blocks_send() {
            return;
        }
        signals.set_error.set(None);
        signals.set_more_menu_open.set(false);
        signals.update_gen(&id, |s| {
            s.stream_error = false;
            s.is_generating = true;
            s.follow_tail = true;
            s.is_self_reply = true;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
        });

        let navigate = navigate.clone();
        let stream_id = id.clone();
        spawn_local(async move {
            let url = format!("/api/chats/{id}/respond-as-user");
            run_stream_generation(url, None, signals, stream_id.clone(), false, navigate.clone()).await;
            finish_stream(signals, stream_id, false, navigate).await;
        });
    }
}
