use leptos::html::Ul;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::collections::HashSet;

use super::chat_helpers::{Thought, RawPromptPanel, walk_active_branch, regeneration_parent_id, speaker_for};

#[component]
pub fn ChatPage() -> impl IntoView {
    let params = use_params_map();
    let chat_id = Memo::new(move |_| params.with(|p| p.get("id").unwrap_or_default()));
    let navigate = use_navigate();
    let navigate_send = navigate.clone();
    let navigate_regen = navigate.clone();
    let navigate_continue = navigate.clone();
    let navigate_self = navigate.clone();

    let (tree, set_tree) = signal(crate::api::MessageTree::default());
    let (chat_meta, set_chat_meta) = signal(Option::<crate::api::Chat>::None);

    let me = LocalResource::new(|| async move { crate::api::fetch_me().await.ok() });
    let settings = LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);

    let all_characters = LocalResource::new(|| async move { crate::api::list_characters().await.unwrap_or_default() });
    let character_by_id = Memo::new(move |_| {
        all_characters.get().unwrap_or_default().into_iter()
            .map(|c| (c.id.clone(), c))
            .collect::<HashMap<String, crate::api::Character>>()
    });
    
    let fetch_tree = {
        let id_for_fetch = chat_id.clone();
        move || {
            let id = id_for_fetch.get();
            spawn_local(async move {
                let mut was_at_bottom = false;
                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                            let scroll_top = el.scroll_top();
                            let client_height = el.client_height();
                            let scroll_height = el.scroll_height();
                            if scroll_height - (scroll_top + client_height) < 150 {
                                was_at_bottom = true;
                            }
                        }
                    }
                }

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
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(50).await;
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                    el.set_scroll_top(el.scroll_height());
                                }
                            }
                        }
                    });
                }
            });
        }
    };
    
    Effect::new({
        let fetch = fetch_tree.clone();
        move |_| fetch()
    });

    let char_name_sig = Signal::derive(move || {
        if let Some(g) = tree.get().group {
            g.group.name
        } else {
            tree.get().character.map(|c| c.name).unwrap_or_else(|| "assistant".to_string())
        }
    });
    
    // the tree doesn't carry the user's display name, so pull it off the /me
    // response we already fetch for the avatar, falling back to username
    // and then a plain "User" if neither is set.
    let user_name_sig = Signal::derive(move || {
        if let Some(m) = me.get().flatten() {
            if let Some(dn) = m.display_name {
                if !dn.trim().is_empty() {
                    return dn;
                }
            }
            if !m.username.trim().is_empty() {
                return m.username;
            }
        }
        "User".to_string()
    });

    let user_avatar_url = Signal::derive(move || me.get().flatten().and_then(|m| m.avatar_url));

    let (show_chat_settings, set_show_chat_settings) = signal(false);
    let (pending_delete_message, set_pending_delete_message) = signal::<Option<String>>(None);
    let (regenerating_msg_id, set_regenerating_msg_id) = signal::<Option<String>>(None);
    let (all_lorebooks, set_all_lorebooks) = signal(Vec::<crate::api::Lorebook>::new());
    let (selected_lorebooks, set_selected_lorebooks) = signal(HashSet::<String>::new());

    Effect::new({
        let chat_id_val = chat_id.clone();
        move |_| {
            let cid = chat_id_val.get();
            spawn_local(async move {
                if let Ok(lbs) = crate::api::list_lorebooks().await {
                    set_all_lorebooks.set(lbs);
                }
                if let Ok(ids) = crate::api::get_chat_lorebooks(&cid).await {
                    set_selected_lorebooks.set(ids.into_iter().collect());
                }
            });
        }
    });

    let save_chat_settings = move |_: leptos::ev::MouseEvent| {
        let cid = chat_id.get_untracked();
        let selected = selected_lorebooks.get_untracked().into_iter().collect::<Vec<_>>();
        spawn_local(async move {
            let _ = crate::api::set_chat_lorebooks(&cid, selected).await;
            set_show_chat_settings.set(false);
        });
    };


    let (selected_children, set_selected_children) =
        signal(HashMap::<String, String>::new());
    let (confirmed_children, set_confirmed_children) =
        signal(HashMap::<String, String>::new());

    let avatar_url = Memo::new(move |_| {
        tree.get().character.and_then(|c| c.avatar_url)
    });

    let active_branch = Memo::new(move |_| {
        walk_active_branch(&tree.get(), &selected_children.get())
    });

    let (greetings, set_greetings) = signal(Vec::<String>::new());
    Effect::new(move |_| {
        let t = tree.get();
        if let Some(c) = t.character {
            spawn_local(async move {
                let mut all = vec![];
                let first = &c.first_message;
                if !first.is_empty() {
                    all.push(first.clone());
                }
                if let Ok(alts) = crate::api::list_alternate_greetings(&c.id).await {
                    for a in alts {
                        all.push(a.greeting);
                    }
                }
                set_greetings.set(all);
            });
        }
    });

    let (greeting_index, set_greeting_index) = signal(0usize);
    let (confirmed_greeting, set_confirmed_greeting) = signal(0usize);

    Effect::new(move |_| {
        let branch = active_branch.get();
        let all = greetings.get();
        if let Some(first) = branch.first() {
            if first.parent_id.is_none() && first.role == "assistant" && all.len() > 1 {
                let idx = all
                    .iter()
                    .position(|g| g.trim() == first.content.trim())
                    .unwrap_or(0);
                set_greeting_index.set(idx);
                set_confirmed_greeting.set(idx);
            }
        }
    });

    let (send_parent_id, set_send_parent_id) = signal(None::<String>);

    let (draft, set_draft) = signal(String::new());
    let (pending_user_text, set_pending_user_text) = signal(Option::<String>::None);
    let (streaming_reply, set_streaming_reply) = signal(String::new());
    let (current_member, set_current_member) = signal(Option::<(String, String)>::None);
    let (group_reply_log, set_group_reply_log) = signal(Vec::<(String, String, String)>::new());
    let (is_generating, set_is_generating) = signal(false);
    let (is_self_reply, set_is_self_reply) = signal(false);
    let (more_menu_open, set_more_menu_open) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (revealed_len, set_revealed_len) = signal(0usize);

    let (editing_id, set_editing_id) = signal(None::<String>);
    let (edit_text, set_edit_text) = signal(String::new());
    let (open_prompt_id, set_open_prompt_id) = signal(None::<String>);

    let message_list_ref = NodeRef::<Ul>::new();
    let (did_initial_scroll, set_did_initial_scroll) = signal(false);

    Effect::new(move |_| {
        active_branch.get();
        if !did_initial_scroll.get_untracked() {
            leptos::task::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                            el.set_scroll_top(el.scroll_height());
                        }
                    }
                }
            });
            set_did_initial_scroll.set(true);
        }
    });

    let extracted_stream = Memo::new(move |_| {
        let full = streaming_reply.get();
        let len = revealed_len.get();
        let prefix: String = if crate::api::get_text_speed() == 0 {
            full
        } else {
            full.chars().take(len).collect()
        };
        crate::render::reasoning::extract_thinking(&prefix)
    });
    let has_thought = Memo::new(move |_| extracted_stream.get().1.is_some());

    let generation_epoch = Rc::new(Cell::new(0u64));

    Effect::new(move |_| {
        if !is_generating.get() {
            return;
        }
        set_revealed_len.set(0);
        let my_epoch = generation_epoch.get() + 1;
        generation_epoch.set(my_epoch);

        let speed = crate::api::get_text_speed();
        if speed == 0 {
            return;
        }
        let interval_ms = 1000 / speed;
        let epoch_for_loop = generation_epoch.clone();

        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(interval_ms).await;
                if !is_generating.get_untracked() || epoch_for_loop.get() != my_epoch {
                    break;
                }
                let total = streaming_reply.get_untracked().chars().count();
                let current = revealed_len.get_untracked();
                if current >= total {
                    continue;
                }
                let behind = total - current;
                let step = if behind > 40 { (behind / 20).max(1) } else { 1 };
                set_revealed_len.set((current + step).min(total));
            }
        });
    });

    let send = {
        move |ev: leptos::ev::SubmitEvent| {
            ev.prevent_default();
            let content = draft.get_untracked();
            if content.is_empty() || is_generating.get_untracked() {
                return;
            }
            set_draft.set(String::new());
            set_error.set(None);
            set_is_generating.set(true);
            set_is_self_reply.set(false);
            set_streaming_reply.set(String::new());
            set_current_member.set(None);
            set_group_reply_log.set(Vec::new());
            set_pending_user_text.set(Some(content.clone()));
            
            leptos::task::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(50).await;
                if let Some(window) = web_sys::window() {
                    if let Some(doc) = window.document() {
                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                            el.set_scroll_top(el.scroll_height());
                        }
                    }
                }
            });

            let id = chat_id.get_untracked();
            // branch point was picked, otherwise every send with no parent
            // starts a brand new root and orphans everything before it.
            let parent = send_parent_id
                .get_untracked()
                .or_else(|| active_branch.get_untracked().last().map(|m| m.id.clone()));
            let body = serde_json::json!({ "content": content, "parent_id": parent }).to_string();
            let navigate = navigate_send.clone();
            spawn_local(async move {
                let url = format!("/api/chats/{id}/generate");
                let result = crate::api::stream_post(
                    &url,
                    Some(body),
                    move |delta| {
                        let mut should_scroll = false;
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                    let scroll_top = el.scroll_top();
                                    let client_height = el.client_height();
                                    let scroll_height = el.scroll_height();
                                    let threshold = scroll_height - (scroll_top + client_height);
                                    if threshold < 150 || scroll_top < 1 {
                                        should_scroll = true;
                                    }
                                }
                            }
                        }
                        
                        set_streaming_reply.update(|current| current.push_str(&delta));

                        if should_scroll {
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(50).await;
                                if let Some(window) = web_sys::window() {
                                    if let Some(doc) = window.document() {
                                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                            el.set_scroll_top(el.scroll_height());
                                        }
                                    }
                                }
                            });
                        }
                    },
                    move |character_id: String, name: String| {
                        let previous = current_member.get_untracked();
                        if let Some((prev_id, prev_name)) = previous {
                            let content = streaming_reply.get_untracked();
                            if !content.is_empty() {
                                set_group_reply_log.update(|log| log.push((prev_id, prev_name, content)));
                            }
                        }
                        set_streaming_reply.set(String::new());
                        set_current_member.set(Some((character_id, name)));
                    },
                    move |err| {
                        if err == crate::api::SESSION_EXPIRED_ERROR {
                            navigate("/login", NavigateOptions::default());
                        } else {
                            set_error.set(Some(err));
                        }
                    },
                )
                .await;
                if let Err(e) = result {
                    set_error.set(Some(e));
                }
                set_is_generating.set(false);
                set_streaming_reply.set(String::new());
                set_current_member.set(None);
                set_group_reply_log.set(Vec::new());
                set_pending_user_text.set(None);
                set_send_parent_id.set(None);
                set_regenerating_msg_id.set(None);
                set_selected_children.set(HashMap::new());
                fetch_tree();
            });
        }
    };

    let regenerate = std::sync::Arc::new({
        let navigate_regen = navigate.clone();
        move || {
            if is_generating.get_untracked() {
                return;
            }
            set_error.set(None);
            set_is_generating.set(true);
            set_is_self_reply.set(false);
            set_streaming_reply.set(String::new());
            set_current_member.set(None);
            set_group_reply_log.set(Vec::new());

            let id = chat_id.get_untracked();
            let branch = active_branch.get_untracked();
            let mut regen_character_id = None;
            if let Some(last) = branch.last() {
                if last.role == "assistant" {
                    set_regenerating_msg_id.set(Some(last.id.clone()));
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
            let group = tree.get_untracked().group;
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
            let navigate = navigate_regen.clone();
            spawn_local(async move {
                let url = format!("/api/chats/{id}/regenerate{parent_param}");
                let result = crate::api::stream_post(
                    &url,
                    None,
                    move |delta| {
                        let mut should_scroll = false;
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                    let scroll_top = el.scroll_top();
                                    let client_height = el.client_height();
                                    let scroll_height = el.scroll_height();
                                    let threshold = scroll_height - (scroll_top + client_height);
                                    if threshold < 150 || scroll_top < 1 {
                                        should_scroll = true;
                                    }
                                }
                            }
                        }
                        
                        set_streaming_reply.update(|current| current.push_str(&delta));

                        if should_scroll {
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(50).await;
                                if let Some(window) = web_sys::window() {
                                    if let Some(doc) = window.document() {
                                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                            el.set_scroll_top(el.scroll_height());
                                        }
                                    }
                                }
                            });
                        }
                    },
                    move |character_id: String, name: String| {
                        let previous = current_member.get_untracked();
                        if let Some((prev_id, prev_name)) = previous {
                            let content = streaming_reply.get_untracked();
                            if !content.is_empty() {
                                set_group_reply_log.update(|log| log.push((prev_id, prev_name, content)));
                            }
                        }
                        set_streaming_reply.set(String::new());
                        set_current_member.set(Some((character_id, name)));
                    },
                    move |err| {
                        if err == crate::api::SESSION_EXPIRED_ERROR {
                            navigate("/login", NavigateOptions::default());
                        } else {
                            set_error.set(Some(err));
                        }
                    },
                )
                .await;
                if let Err(e) = result {
                    set_error.set(Some(e));
                }
                set_is_generating.set(false);
                set_streaming_reply.set(String::new());
                set_current_member.set(None);
                set_group_reply_log.set(Vec::new());
                set_regenerating_msg_id.set(None);
                set_selected_children.set(HashMap::new());
                fetch_tree();
            });
        }
    });

    let continue_gen = {
        move |_| {
            if is_generating.get_untracked() {
                return;
            }
            set_error.set(None);
            set_is_generating.set(true);
            set_is_self_reply.set(false);
            set_more_menu_open.set(false);

            let id = chat_id.get_untracked();
            let navigate = navigate_continue.clone();
            spawn_local(async move {
                let url = format!("/api/chats/{id}/continue");
                let result = crate::api::stream_post(
                    &url,
                    None,
                    move |delta| {
                        let mut should_scroll = false;
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                    let scroll_top = el.scroll_top();
                                    let client_height = el.client_height();
                                    let scroll_height = el.scroll_height();
                                    let threshold = scroll_height - (scroll_top + client_height);
                                    if threshold < 150 || scroll_top < 1 {
                                        should_scroll = true;
                                    }
                                }
                            }
                        }
                        
                        set_streaming_reply.update(|current| current.push_str(&delta));

                        if should_scroll {
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(50).await;
                                if let Some(window) = web_sys::window() {
                                    if let Some(doc) = window.document() {
                                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                            el.set_scroll_top(el.scroll_height());
                                        }
                                    }
                                }
                            });
                        }
                    },
                    move |_character_id: String, _name: String| {},
                    move |err| {
                        if err == crate::api::SESSION_EXPIRED_ERROR {
                            navigate("/login", NavigateOptions::default());
                        } else {
                            set_error.set(Some(err));
                        }
                    },
                )
                .await;
                if let Err(e) = result {
                    set_error.set(Some(e));
                }
                set_is_generating.set(false);
                set_streaming_reply.set(String::new());
                set_current_member.set(None);
                set_group_reply_log.set(Vec::new());
                fetch_tree();
            });
        }
    };

    let respond_as_me = {
        move |_| {
            if is_generating.get_untracked() {
                return;
            }
            set_error.set(None);
            set_is_generating.set(true);
            set_is_self_reply.set(true);
            set_more_menu_open.set(false);
            set_streaming_reply.set(String::new());
            set_current_member.set(None);
            set_group_reply_log.set(Vec::new());

            let id = chat_id.get_untracked();
            let navigate = navigate_self.clone();
            spawn_local(async move {
                let url = format!("/api/chats/{id}/respond-as-user");
                let result = crate::api::stream_post(
                    &url,
                    None,
                    move |delta| {
                        let mut should_scroll = false;
                        if let Some(window) = web_sys::window() {
                            if let Some(doc) = window.document() {
                                if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                    let scroll_top = el.scroll_top();
                                    let client_height = el.client_height();
                                    let scroll_height = el.scroll_height();
                                    let threshold = scroll_height - (scroll_top + client_height);
                                    if threshold < 150 || scroll_top < 1 {
                                        should_scroll = true;
                                    }
                                }
                            }
                        }

                        set_streaming_reply.update(|current| current.push_str(&delta));

                        if should_scroll {
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(50).await;
                                if let Some(window) = web_sys::window() {
                                    if let Some(doc) = window.document() {
                                        if let Ok(Some(el)) = doc.query_selector(".main-content") {
                                            el.set_scroll_top(el.scroll_height());
                                        }
                                    }
                                }
                            });
                        }
                    },
                    move |_character_id: String, _name: String| {},
                    move |err| {
                        if err == crate::api::SESSION_EXPIRED_ERROR {
                            navigate("/login", NavigateOptions::default());
                        } else {
                            set_error.set(Some(err));
                        }
                    },
                )
                .await;
                if let Err(e) = result {
                    set_error.set(Some(e));
                }
                set_is_generating.set(false);
                set_is_self_reply.set(false);
                set_streaming_reply.set(String::new());
                set_current_member.set(None);
                set_group_reply_log.set(Vec::new());
                set_selected_children.set(HashMap::new());
                fetch_tree();
            });
        }
    };

    let delete = move |message_id: String| {
        spawn_local(async move {
            match crate::api::delete_message(&message_id).await {
                Ok(()) => {
                    set_error.set(None);
                    fetch_tree();
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let delete_all_below = move |message_id: String| {
        let branch = active_branch.get_untracked();
        if let Some(idx) = branch.iter().position(|m| m.id == message_id) {
            let to_delete: Vec<String> = branch[idx..].iter().map(|m| m.id.clone()).collect();
            spawn_local(async move {
                for id in to_delete.into_iter().rev() {
                    let _ = crate::api::delete_message(&id).await;
                }
                set_error.set(None);
                fetch_tree();
            });
        }
    };

    let toggle_visibility = move |message_id: String, visible: bool| {
        spawn_local(async move {
            if let Err(e) = crate::api::set_message_visibility(&message_id, !visible).await {
                set_error.set(Some(e));
            } else {
                fetch_tree();
            }
        });
    };

    let save_edit = move |message_id: String| {
        let content = edit_text.get_untracked();
        spawn_local(async move {
            if let Err(e) = crate::api::edit_message(&message_id, &content).await {
                set_error.set(Some(e));
            } else {
                set_editing_id.set(None);
                fetch_tree();
            }
        });
    };

    let select_sibling = move |parent_id: String, child_id: String| {
        let cid = chat_id.get_untracked();
        set_selected_children.update(|map| {
            map.insert(parent_id.clone(), child_id.clone());
        });
        spawn_local(async move {
            if let Ok(extra) = crate::api::subtree(&cid, &child_id, 50).await {
                set_tree.update(|t| {
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
    };

    let select_branch_point = move |message_id: String| {
        let current = send_parent_id.get();
        let new_val = if current.as_ref() == Some(&message_id) {
            None
        } else {
            Some(message_id)
        };
        set_send_parent_id.set(new_val);
    };

    let leaf_is_assistant = Memo::new(move |_| {
        active_branch
            .get()
            .last()
            .is_some_and(|n| n.role == "assistant")
    });

    let sibling_info = Memo::new(move |_| {
        let t = tree.get();
        let mut info: HashMap<String, (String, usize, usize)> = HashMap::new();
        for (id, node) in &t.messages {
            if let Some(parent_id) = &node.parent_id {
                if let Some(parent) = t.messages.get(parent_id) {
                    if parent.children.len() > 1 {
                        let idx = parent.children.iter().position(|c| c == id).unwrap_or(0);
                        info.insert(id.clone(), (parent_id.clone(), parent.children.len(), idx));
                    }
                }
            }
        }
        info
    });

    // name/avatar for the in-progress streaming bubble. in a group chat
    // current_member tracks whoever's turn it currently is; outside of a
    // group round (or in a 1:1 chat, where current_member never gets set)
    // this just falls back to today's single-character behavior. for a
    // group chat specifically, current_member being None still means "no
    // member event has landed yet" (there's a beat between is_generating
    // flipping true and the first event: member), so stay blank there
    // instead of showing the group's own name as if it were a speaker.
    let streaming_speaker = Signal::derive(move || {
        match current_member.get() {
            Some((cid, name)) => {
                let (_, avatar) = speaker_for(&character_by_id.get(), &None, &Some(cid));
                (name, avatar)
            }
            None if tree.get().group.is_some() => (String::new(), None),
            None => (char_name_sig.get(), avatar_url.get()),
        }
    });

    view! {
        <div class="chat-page">

        <div class="chat-header" style="position: sticky; top: -2rem; z-index: 100; background: #0a0a0c; display: flex; justify-content: space-between; align-items: center; padding: 1rem 0; margin-top: -2rem; margin-bottom: 1.5rem; border-bottom: 1px solid var(--color-border);">
                <strong>{move || char_name_sig.get()}</strong>
                <div style="display: flex; align-items: center; gap: 0.5rem;">
                    // mounted once behind Show rather than rebuilt from chat_meta.get().map(...)
                    // on every render. chat_meta flips from None to Some exactly once per page
                    // load and then just updates in place, so ChatMembers keeps its own
                    // show_panel state open across the fetch_tree() that follows every action
                    // (add/remove/reorder/rename/strategy) instead of getting torn down and
                    // remounted, which used to close the panel after a single click
                    <Show when=move || chat_meta.get().is_some()>
                        <crate::components::chat_members::ChatMembers
                            chat=Signal::derive(move || chat_meta.get().unwrap_or_default())
                            group=Signal::derive(move || tree.get().group.clone())
                            all_characters=Signal::derive(move || all_characters.get().unwrap_or_default())
                            on_change=Callback::new(move |_| fetch_tree())
                        />
                    </Show>
                    <button class="icon-btn" title="Chat Settings" on:click=move |_| set_show_chat_settings.set(true)>
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="12" r="3"></circle>
                            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
                        </svg>
                    </button>
                </div>
            </div>
            <ul class="message-list" node_ref=message_list_ref>
                {
                    let regenerate_outer = regenerate.clone();
                    move || {
                    let regenerate_inner = regenerate_outer.clone();
                    let branch = active_branch.get();
                    let editing = editing_id.get();
                    let send_pid = send_parent_id.get();
                    let sib_info = sibling_info.get();
                    let last_id = branch.iter().filter(|n| !n.deleted).last().map(|n| n.id.clone());
                    let char_map = character_by_id.get();
                    let fallback_character = tree.get_untracked().character.clone();
                    let is_group_chat = tree.get_untracked().group.is_some();

                    branch
                        .into_iter()
                        .filter(|n| !n.deleted)
                        .map(move |message| {
                            let id = message.id.clone();
                            let role = message.role.clone();
                            let content = if is_generating.get() && regenerating_msg_id.get() == Some(id.clone()) {
                                streaming_reply.get()
                            } else {
                                message.content.clone()
                            };
                            let visible = message.visible;
                            let raw_prompt = message.raw_prompt.clone();
                            let (speaker_name, speaker_avatar) =
                                speaker_for(&char_map, &fallback_character, &message.character_id);

                            let (visible_body, thought) =
                                crate::render::reasoning::extract_thinking(&content);
                            let rendered =
                                crate::render::markdown::render_markdown(&visible_body, &speaker_name, &user_name_sig.get(), forbid_media());

                            let is_selected_branch_point = send_pid.as_ref() == Some(&id);
                            let is_editing = editing.as_ref() == Some(&id);
                            
                            let is_greeting_msg = message.parent_id.is_none() && role == "assistant";
                            let all_greetings = greetings.get();
                            let greeting_idx = greeting_index.get();
                            let confirmed_idx = confirmed_greeting.get();
                            let greeting_total = all_greetings.len();

                            let sibling_display = sib_info.get(&id).map(|(pid, count, idx)| {
                                let parent = pid.clone();
                                let total = *count;
                                let current = *idx + 1;
                                let prev_child = if *idx > 0 {
                                    let t = tree.get_untracked();
                                    t.messages.get(&parent).and_then(|p| p.children.get(idx - 1)).cloned()
                                } else { None };
                                let next_child = if *idx + 1 < total {
                                    let t = tree.get_untracked();
                                    t.messages.get(&parent).and_then(|p| p.children.get(idx + 1)).cloned()
                                } else { None };
                                (parent.clone(), current, total, prev_child, next_child)
                            });

                            view! {
                                <li class=format!(
                                    "message-row {} {} {}",
                                    role,
                                    if !visible { "dimmed" } else { "" },
                                    if is_selected_branch_point { "branch-selected" } else { "" },
                                )>
                                    {
                                        let label_role = role.clone();
                                        let label_speaker_name = speaker_name.clone();
                                        move || {
                                            if is_group_chat && label_role == "assistant" {
                                                view! { <span class="message-speaker-name">{label_speaker_name.clone()}</span> }.into_any()
                                            } else {
                                                ().into_any()
                                            }
                                        }
                                    }
                                    {if role == "assistant" {
                                        let speaker_avatar_for_view = speaker_avatar.clone();
                                        view! {
                                            {move || {
                                                speaker_avatar_for_view.clone().filter(|url| !url.is_empty() && !forbid_media()).map(|url| {
                                                    view! {
                                                        <div class="message-avatar-rect">
                                                            <img src=url alt="avatar" />
                                                        </div>
                                                    }
                                                })
                                            }}
                                        }.into_any()
                                    } else {
                                        view! {
                                            {move || {
                                                user_avatar_url.get().filter(|url| !url.is_empty() && !forbid_media()).map(|url| {
                                                    view! {
                                                        <div class="message-avatar-rect">
                                                            <img src=url alt="avatar" />
                                                        </div>
                                                    }
                                                })
                                            }}
                                        }.into_any()
                                    }}
                                <div class="message-content-wrapper">
                                    <div class="message-header">
                                        <strong>
                                            {if role == "assistant" {
                                                speaker_name.clone()
                                            } else {
                                                user_name_sig.get()
                                            }}
                                        </strong>
                                    </div>
                                    <div
                                        class="message-body"
                                    >
                                        {if is_editing {
                                            view! {
                                                <textarea
                                                    class="edit-textarea"
                                                    prop:value=edit_text
                                                    on:input=move |ev| set_edit_text.set(event_target_value(&ev))
                                                ></textarea>
                                                <div class="edit-actions">
                                                    <button
                                                        class="ghost"
                                                        on:click={
                                                            let id = id.clone();
                                                            move |_| save_edit(id.clone())
                                                        }
                                                    >"Save"</button>
                                                    <button
                                                        class="ghost"
                                                        on:click=move |_| set_editing_id.set(None)
                                                    >"Cancel"</button>
                                                </div>
                                            }.into_any()
                                        } else if is_greeting_msg {
                                            let content_fallback = content.clone();
                                            view! {
                                                {thought.map(|t| view! { <Thought text=t forbid_media=forbid_media() /> })}
                                                <div>{move || {
                                                    let all = greetings.get();
                                                    let idx = greeting_index.get();
                                                    let conf = confirmed_greeting.get();
                                                    let content_str = if idx == conf {
                                                        content_fallback.as_str()
                                                    } else {
                                                        all.get(idx).map(|s| s.as_str()).unwrap_or(&content_fallback)
                                                    };
                                                    let (body, _) = crate::render::reasoning::extract_thinking(content_str);
                                                    crate::render::markdown::render_markdown(&body, &speaker_name, &user_name_sig.get(), forbid_media())
                                                }}</div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                {thought.map(|t| view! { <Thought text=t forbid_media=forbid_media() /> })}
                                                <div>{rendered}</div>
                                            }.into_any()
                                        }}
                                    </div>
                                    <div class="message-actions">
                                        {if is_greeting_msg && greeting_total > 0 {
                                            let id_confirm = id.clone();
                                            let id_cancel = id.clone();
                                            let all_for_confirm = all_greetings.clone();
                                            let all_for_cancel = all_greetings.clone();
                                            let prev_idx = if greeting_idx == 0 { greeting_total.saturating_sub(1) } else { greeting_idx - 1 };
                                            let next_idx = if greeting_idx + 1 >= greeting_total { 0 } else { greeting_idx + 1 };
                                            let is_dirty = greeting_idx != confirmed_idx;
                                            let _ = is_dirty;
                                            view! {
                                                <span class="sibling-nav greeting-nav">
                                                    <button
                                                        class="ghost sibling-arrow"
                                                        title="Previous greeting"
                                                        on:click=move |_| {
                                                            set_greeting_index.set(prev_idx);
                                                        }
                                                    >"←"</button>
                                                    <span class="sibling-counter">{move || format!("{}/{}", greeting_index.get() + 1, all_greetings.len())}</span>
                                                    <button
                                                        class="ghost sibling-arrow"
                                                        title="Next greeting"
                                                        on:click=move |_| {
                                                            set_greeting_index.set(next_idx);
                                                        }
                                                    >"→"</button>
                                                    {move || if greeting_index.get() != confirmed_greeting.get() {
                                                        let idx = greeting_index.get();
                                                        let conf = confirmed_greeting.get();
                                                        let all_c = all_for_confirm.clone();
                                                        let id_c = id_confirm.clone();
                                                        let all_x = all_for_cancel.clone();
                                                        let id_x = id_cancel.clone();
                                                        view! {
                                                            <button
                                                                class="ghost greeting-confirm"
                                                                title="Confirm greeting"
                                                                on:click=move |_| {
                                                                    let content = all_c.get(idx).cloned();
                                                                    if let Some(content) = content {
                                                                        let id = id_c.clone();
                                                                        spawn_local(async move {
                                                                            let _ = crate::api::edit_message(&id, &content).await;
                                                                            fetch_tree();
                                                                        });
                                                                    }
                                                                    set_confirmed_greeting.set(idx);
                                                                }
                                                            >"✓"</button>
                                                            <button
                                                                class="ghost greeting-cancel"
                                                                title="Revert greeting"
                                                                on:click=move |_| {
                                                                    set_greeting_index.set(conf);
                                                                    let _ = id_x;
                                                                    let _ = all_x;
                                                                }
                                                            >"✕"</button>
                                                        }.into_any()
                                                    } else {
                                                        ().into_any()
                                                    }}
                                                </span>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        }}
                                        {sibling_display.map(|(pid, current, total, prev, next)| {
                                                let pid1 = pid.clone();
                                                let pid2 = pid.clone();
                                                let pid_c = pid.clone();
                                                let pid_x = pid.clone();
                                                let current_id = id.clone();
                                                let id_c = id.clone();
                                                view! {
                                                    <span class="sibling-nav">
                                                        {if let Some(child_id) = prev {
                                                            view! {
                                                                <button
                                                                    class="ghost sibling-arrow"
                                                                    on:click={
                                                                        let pid = pid1.clone();
                                                                        let child_id = child_id.clone();
                                                                        move |_| select_sibling(pid.clone(), child_id.clone())
                                                                    }
                                                                >"←"</button>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span class="sibling-arrow placeholder">" "</span> }.into_any()
                                                        }}
                                                        <span class="sibling-counter">{format!("{current}/{total}")}</span>
                                                        {if let Some(child_id) = next {
                                                            view! {
                                                                <button
                                                                    class="ghost sibling-arrow"
                                                                    on:click={
                                                                        let pid = pid2.clone();
                                                                        let child_id = child_id.clone();
                                                                        move |_| select_sibling(pid.clone(), child_id.clone())
                                                                    }
                                                                >"→"</button>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span class="sibling-arrow placeholder">" "</span> }.into_any()
                                                        }}
                                                        {move || {
                                                            let conf_id = confirmed_children.get().get(&pid_c).cloned();
                                                            let is_dirty = if let Some(c) = &conf_id {
                                                                c != &current_id
                                                            } else {
                                                                current != total
                                                            };
                                                            if is_dirty {
                                                                let pc = pid_c.clone();
                                                                let px = pid_x.clone();
                                                                let ic = id_c.clone();
                                                                view! {
                                                                    <button
                                                                        class="ghost greeting-confirm"
                                                                        title="Confirm version"
                                                                        on:click=move |_| {
                                                                            set_confirmed_children.update(|map| {
                                                                                map.insert(pc.clone(), ic.clone());
                                                                            });
                                                                        }
                                                                    >"✓"</button>
                                                                    <button
                                                                        class="ghost greeting-cancel"
                                                                        title="Revert version"
                                                                        on:click=move |_| {
                                                                            let p = px.clone();
                                                                            let revert_id = conf_id.clone();
                                                                            if let Some(rid) = revert_id {
                                                                                select_sibling(p, rid);
                                                                            } else {
                                                                                // revert to default (last child)
                                                                                let t = tree.get_untracked();
                                                                                if let Some(parent_node) = t.messages.get(&p) {
                                                                                    if let Some(last_child) = parent_node.children.last() {
                                                                                        select_sibling(p, last_child.clone());
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    >"✕"</button>
                                                                }.into_any()
                                                            } else {
                                                                ().into_any()
                                                            }
                                                        }}
                                                    </span>
                                                }
                                            })}
                                        <span class="actions-spacer"></span>
                                        <button
                                            class="icon-btn visibility-toggle"
                                            title=move || if visible { "Visible in context" } else { "Hidden from context" }
                                                on:click={
                                                    let id = id.clone();
                                                    move |_| toggle_visibility(id.clone(), visible)
                                                }
                                            >
                                                {if visible {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path>
                                                            <circle cx="12" cy="12" r="3"></circle>
                                                        </svg>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                            <path d="M17.94 17.94A10.94 10.94 0 0 1 12 20c-7 0-11-8-11-8a18.5 18.5 0 0 1 5.06-5.94M9.9 4.24A10.94 10.94 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path>
                                                            <line x1="1" y1="1" x2="23" y2="23"></line>
                                                        </svg>
                                                    }.into_any()
                                                }}
                                            </button>
                                            <button
                                                class="icon-btn edit-message"
                                                title="Edit message"
                                                on:click={
                                                    let id = id.clone();
                                                    let content = content.clone();
                                                    let is_greet = is_greeting_msg;
                                                    move |_| {
                                                        set_editing_id.set(Some(id.clone()));
                                                        if is_greet {
                                                            let all = greetings.get_untracked();
                                                            let idx = greeting_index.get_untracked();
                                                            let conf = confirmed_greeting.get_untracked();
                                                            if idx == conf {
                                                                set_edit_text.set(content.clone());
                                                            } else {
                                                                set_edit_text.set(all.get(idx).cloned().unwrap_or(content.clone()));
                                                            }
                                                        } else {
                                                            set_edit_text.set(content.clone());
                                                        }
                                                    }
                                                }
                                            >
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                    <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"></path>
                                                </svg>
                                            </button>
                                            {raw_prompt.clone().map(|_| {
                                                let id_for_toggle = id.clone();
                                                view! {
                                                    <button
                                                        class="icon-btn view-prompt"
                                                        title="View the raw prompt sent for this message"
                                                        on:click=move |_| {
                                                            set_open_prompt_id.update(|current| {
                                                                *current = if current.as_deref() == Some(id_for_toggle.as_str()) {
                                                                    None
                                                                } else {
                                                                    Some(id_for_toggle.clone())
                                                                };
                                                            });
                                                        }
                                                    >
                                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                            <polyline points="4 17 10 11 4 5"></polyline>
                                                            <line x1="12" y1="19" x2="20" y2="19"></line>
                                                        </svg>
                                                    </button>
                                                }
                                            })}
                                            <button
                                                class="icon-btn message-delete"
                                                title="Delete message"
                                                on:click={
                                                    let id = id.clone();
                                                    move |_| set_pending_delete_message.set(Some(id.clone()))
                                                }
                                            >
                                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                    <polyline points="3 6 5 6 21 6"></polyline>
                                                    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path>
                                                    <line x1="10" y1="11" x2="10" y2="17"></line>
                                                    <line x1="14" y1="11" x2="14" y2="17"></line>
                                                </svg>
                                            </button>
                                            {
                                                let is_last = Some(id.clone()) == last_id;
                                                let is_gen = is_generating.get();
                                                let regen = regenerate_inner.clone();
                                                if is_last {
                                                    view! {
                                                        <button
                                                            class="icon-btn regenerate-message"
                                                            title="Regenerate"
                                                            disabled=is_gen
                                                            on:click=move |_| regen()
                                                        >
                                                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                                <path d="M21 2v6h-6"></path>
                                                                <path d="M3 12a9 9 0 1 0 2-7l5 5"></path>
                                                            </svg>
                                                        </button>
                                                    }.into_any()
                                                } else {
                                                    view! { <span style="display: none;"></span> }.into_any()
                                                }
                                            }
                                    </div>
                                </div>
                            </li>
                            }
                        })
                        .collect_view()
                }}
                <Show when=move || is_generating.get() && regenerating_msg_id.get().is_none()>
                    {move || {
                        pending_user_text.get().map(|text| {
                            view! {
                                <li class="message-row user">
                                    {move || {
                                        user_avatar_url.get().filter(|url| !url.is_empty() && !forbid_media()).map(|url| {
                                            view! {
                                                <div class="message-avatar-rect">
                                                    <img src=url alt="avatar" />
                                                </div>
                                            }
                                        })
                                    }}
                                    <div class="message-content-wrapper">
                                        <div class="message-header"><strong>{user_name_sig.get()}</strong></div>
                                        <div class="message-body">
                                            <div>{crate::render::markdown::render_markdown(&text, "", "", forbid_media())}</div>
                                        </div>
                                    </div>
                                </li>
                            }
                        })
                    }}
                    {move || {
                        if tree.get().group.is_none() {
                            return ().into_any();
                        }
                        let char_map = character_by_id.get();
                        group_reply_log.get().into_iter().map(|(character_id, name, content)| {
                            let (_, avatar) = speaker_for(&char_map, &None, &Some(character_id));
                            let (visible_body, thought) = crate::render::reasoning::extract_thinking(&content);
                            let rendered = crate::render::markdown::render_markdown(&visible_body, &name, &user_name_sig.get(), forbid_media());
                            view! {
                                <li class="message-row assistant">
                                    {avatar.filter(|url| !url.is_empty() && !forbid_media()).map(|url| view! {
                                        <div class="message-avatar-rect"><img src=url alt="avatar" /></div>
                                    })}
                                    <div class="message-content-wrapper">
                                        <div class="message-header"><strong>{name}</strong></div>
                                        <div class="message-body">
                                            {thought.map(|t| view! { <Thought text=t forbid_media=forbid_media() /> })}
                                            <div>{rendered}</div>
                                        </div>
                                    </div>
                                </li>
                            }
                        }).collect::<Vec<_>>().into_any()
                    }}
                    <li class=move || if is_self_reply.get() { "message-row user" } else { "message-row assistant" }>
                        {move || {
                            if is_self_reply.get() {
                                user_avatar_url.get().filter(|url| !url.is_empty() && !forbid_media()).map(|url| {
                                    view! {
                                        <div class="message-avatar-rect">
                                            <img src=url alt="avatar" />
                                        </div>
                                    }
                                })
                            } else {
                                streaming_speaker.get().1.filter(|url| !url.is_empty() && !forbid_media()).map(|url| {
                                    view! {
                                        <div class="message-avatar-rect">
                                            <img src=url alt="avatar" />
                                        </div>
                                    }
                                })
                            }
                        }}
                        <div class="message-content-wrapper">
                            <div class="message-header">
                                <strong>{move || if is_self_reply.get() { user_name_sig.get() } else { streaming_speaker.get().0 }}</strong>
                            </div>
                            <div class="message-body">
                                <Show when=move || has_thought.get()>
                                    <Thought forbid_media=forbid_media() text=Signal::derive(move || {
                                        extracted_stream.get().1.unwrap_or_default()
                                    }) />
                                </Show>
                                <div>
                                    {move || {
                                        crate::render::markdown::render_markdown(
                                            &extracted_stream.get().0,
                                            "",
                                            "",
                                            forbid_media()
                                        )
                                    }}
                                </div>
                                <span class="streaming-cursor">"|"</span>
                            </div>
                        </div>
                    </li>
                </Show>
            </ul>



            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}

            <form class="chat-input-bar" on:submit=send>
                {move || {
                    if is_generating.get() {
                        ().into_any()
                    } else {
                        let continue_gen = continue_gen.clone();
                        let respond_as_me = respond_as_me.clone();
                        view! {
                            <div class="more-menu-wrap">
                                <button
                                    type="button"
                                    class="ghost icon-btn"
                                    title="More generation options"
                                    on:click=move |_| set_more_menu_open.update(|o| *o = !*o)
                                >
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                        <circle cx="12" cy="5" r="1.5"></circle>
                                        <circle cx="12" cy="12" r="1.5"></circle>
                                        <circle cx="12" cy="19" r="1.5"></circle>
                                    </svg>
                                </button>
                                {move || {
                                    if !more_menu_open.get() {
                                        ().into_any()
                                    } else {
                                        let continue_item = if leaf_is_assistant.get() {
                                            view! {
                                                <button
                                                    type="button"
                                                    class="more-menu-item"
                                                    on:click=continue_gen.clone()
                                                >
                                                    "▶ Continue generation"
                                                </button>
                                            }.into_any()
                                        } else {
                                            ().into_any()
                                        };
                                        view! {
                                            <div class="more-menu-backdrop" on:click=move |_| set_more_menu_open.set(false)></div>
                                            <div class="more-menu-panel">
                                                {continue_item}
                                                <button
                                                    type="button"
                                                    class="more-menu-item"
                                                    on:click=respond_as_me.clone()
                                                >
                                                    "Respond as Me"
                                                </button>
                                            </div>
                                        }.into_any()
                                    }
                                }}
                            </div>
                        }.into_any()
                    }
                }}

                <input
                    type="text"
                    placeholder="Type a message..."
                    prop:value=draft
                    on:input=move |ev| set_draft.set(event_target_value(&ev))
                />
                <button class="primary send-btn" type="submit" title="Send" disabled=move || is_generating.get()>
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="22" y1="2" x2="11" y2="13"></line>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
                    </svg>
                </button>
            </form>

            {move || {
                open_prompt_id.get().and_then(|open_id| {
                    tree.get().messages.get(&open_id).cloned()
                }).map(|node| {
                    let crate::api::MessageNode { raw_prompt, prompt_tokens, context_limit, .. } = node;
                    view! {
                        <div class="modal-backdrop" on:click=move |_| set_open_prompt_id.set(None)>
                            <div class="modal-box" on:click=|ev| ev.stop_propagation()>
                                <div class="modal-header">
                                    <strong>"Raw prompt"</strong>
                                    <button
                                        class="icon-btn"
                                        title="Close"
                                        on:click=move |_| set_open_prompt_id.set(None)
                                    >
                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <line x1="18" y1="6" x2="6" y2="18"></line>
                                            <line x1="6" y1="6" x2="18" y2="18"></line>
                                        </svg>
                                    </button>
                                </div>
                                {raw_prompt.map(|raw| view! {
                                    <RawPromptPanel raw_prompt=raw prompt_tokens=prompt_tokens context_limit=context_limit />
                                })}
                            </div>
                        </div>
                    }
                })
            }}

            {move || {
                if show_chat_settings.get() {
                    view! {
                        <div class="modal-backdrop" on:click=move |_| set_show_chat_settings.set(false)>
                            <div class="modal-box" on:click=|ev| ev.stop_propagation()>
                                <div class="modal-header">
                                    <strong>"Chat Settings"</strong>
                                    <button
                                        class="icon-btn"
                                        title="Close"
                                        on:click=move |_| set_show_chat_settings.set(false)
                                    >
                                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <line x1="18" y1="6" x2="6" y2="18"></line>
                                            <line x1="6" y1="6" x2="18" y2="18"></line>
                                        </svg>
                                    </button>
                                </div>
                                <div class="modal-body" style="padding: 1rem;">
                                    <label style="margin-bottom: 0.5rem; display: block; font-weight: 500;">"Attached Grimoires (Chat-specific)"</label>
                                    <div style="display: flex; flex-direction: column; gap: 0.5rem; max-height: 400px; overflow-y: auto; padding: 0.5rem; border: 1px solid var(--color-border); margin-bottom: 1rem;">
                                        {all_lorebooks.get().into_iter().map(|lb| {
                                            let id = lb.id.clone();
                                            let id2 = lb.id.clone();
                                            let id3 = lb.id.clone();
                                            let id4 = lb.id.clone();
                                            
                                            view! {
                                                <label style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; border: 1px solid var(--color-border); cursor: pointer; transition: background 0.2s;"
                                                    class:active=move || selected_lorebooks.get().contains(&id3)>
                                                    <input type="checkbox" 
                                                        prop:checked=move || selected_lorebooks.get().contains(&id4)
                                                        on:change=move |ev| {
                                                            let checked = event_target_checked(&ev);
                                                            set_selected_lorebooks.update(|set: &mut HashSet<String>| {
                                                                if checked {
                                                                    set.insert(id2.clone());
                                                                } else {
                                                                    set.remove(&id2);
                                                                }
                                                            });
                                                        }
                                                    />
                                                    <div style="display: flex; flex-direction: column;">
                                                        <span style="font-weight: 500; font-family: var(--font-heading);">{lb.name.clone()}</span>
                                                        <span style="font-size: 0.8rem; color: var(--color-text-muted);">{lb.description.clone()}</span>
                                                    </div>
                                                </label>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                    <button class="btn primary" on:click=save_chat_settings.clone()>"Save"</button>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    ().into_any()
                }
            }}


            {move || if let Some(id) = pending_delete_message.get() {
                let id_for_single = id.clone();
                let id_for_all = id.clone();
                view! {
                    <div class="modal-backdrop" on:click=move |_| set_pending_delete_message.set(None)>
                        <div class="modal-box" style="max-width: 400px;" on:click=|ev| ev.stop_propagation()>
                            <div class="modal-header">
                                <h2>"Delete Message"</h2>
                                <button class="icon-btn" on:click=move |_| set_pending_delete_message.set(None)>
                                    <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2">
                                        <line x1="18" y1="6" x2="6" y2="18"></line>
                                        <line x1="6" y1="6" x2="18" y2="18"></line>
                                    </svg>
                                </button>
                            </div>
                            <div class="modal-body" style="padding: 1rem; display: flex; flex-direction: column; gap: 1rem;">
                                <p>"Do you want to delete just this message, or this message and all messages following it in this branch?"</p>
                                <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                                    <button class="btn secondary" on:click=move |_| {
                                        delete(id_for_single.clone());
                                        set_pending_delete_message.set(None);
                                    }>"Delete just this message"</button>
                                    <button class="btn danger" style="background: var(--color-error); color: white;" on:click=move |_| {
                                        delete_all_below(id_for_all.clone());
                                        set_pending_delete_message.set(None);
                                    }>"Delete this and all below it"</button>
                                </div>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}
