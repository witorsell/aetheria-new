use leptos::html::Ul;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::collections::HashSet;

use super::chat_actions::{
    build_delete, build_delete_all_below, build_save_edit, build_select_branch_point,
    build_select_sibling, build_toggle_visibility,
};
use super::chat_data::build_fetch_tree;
use super::chat_generation::{build_continue_gen, build_regenerate, build_respond_as_me, build_send};
use super::chat_helpers::{Thought, RawPromptPanel, walk_active_branch, speaker_for};
use super::chat_state::{install_tail_follow_break, maintain_tail_follow, ChatSignals, GenState};

/// Tap a message to reveal its tools. Buttons inside the row keep the click.
fn focus_message_row(ev: web_sys::MouseEvent) {
    use wasm_bindgen::JsCast;
    let Some(target) = ev.target() else { return };
    let Ok(target) = target.dyn_into::<web_sys::Element>() else { return };
    if target.closest("button, a, textarea, input, label").ok().flatten().is_some() {
        return;
    }
    let Some(current) = ev.current_target() else { return };
    if let Ok(el) = current.dyn_into::<web_sys::HtmlElement>() {
        let _ = el.focus();
    }
}

#[component]
pub fn ChatPage() -> impl IntoView {
    let params = use_params_map();
    let chat_id = Memo::new(move |_| params.with(|p| p.get("id").unwrap_or_default()));
    let navigate = use_navigate();

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
    
    let char_name_sig = Signal::derive(move || {
        if let Some(g) = tree.get().group {
            g.group.name
        } else {
            tree.get().character.map(|c| c.name).unwrap_or_else(|| "assistant".to_string())
        }
    });
    
    // the tree doesn't carry the user's display name, so pull it off the /me
    // response we already fetch for the avatar. an active persona overrides
    // the account name for {{user}}, same fallback order the backend uses
    // in fetch_user_persona: persona name, then display name, then username,
    // then a plain "User" if none of those are set.
    let user_name_sig = Signal::derive(move || {
        if let Some(m) = me.get().flatten() {
            if let Some(pn) = m.persona_name {
                if !pn.trim().is_empty() {
                    return pn;
                }
            }
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

    let user_avatar_url = Signal::derive(move || {
        me.get().flatten().and_then(|m| m.persona_avatar_url.or(m.avatar_url))
    });

    let (show_chat_settings, set_show_chat_settings) = signal(false);
    let (pending_delete_message, set_pending_delete_message) = signal::<Option<String>>(None);
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

    let (error, set_error) = signal(Option::<String>::None);

    let save_chat_settings = move |_: leptos::ev::MouseEvent| {
        let cid = chat_id.get_untracked();
        let selected = selected_lorebooks.get_untracked().into_iter().collect::<Vec<_>>();
        spawn_local(async move {
            match crate::api::set_chat_lorebooks(&cid, selected).await {
                Ok(()) => set_show_chat_settings.set(false),
                Err(e) => set_error.set(Some(e)),
            }
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

    // keyed by chat id rather than flat page-level signals - ChatPage is a
    // single persistent instance reused across /chat/A -> /chat/B
    // navigation, so a flat is_generating/streaming_reply would let a
    // stream started in chat A keep writing into whatever chat is on
    // screen when its deltas arrive. see chat_state::GenState.
    let gen_states = RwSignal::new(HashMap::<String, GenState>::new());
    let outbox = RwSignal::new(HashMap::<String, Vec<String>>::new());
    let gen_for_current = Memo::new(move |_| {
        gen_states.with(|m| m.get(&chat_id.get()).cloned().unwrap_or_default())
    });
    let is_generating = Memo::new(move |_| gen_for_current.get().is_generating);
    let streaming_reply = Memo::new(move |_| gen_for_current.get().streaming_reply);
    let current_member = Memo::new(move |_| gen_for_current.get().current_member);
    let group_reply_log = Memo::new(move |_| gen_for_current.get().group_reply_log);
    let stream_error = Memo::new(move |_| gen_for_current.get().stream_error);
    let is_self_reply = Memo::new(move |_| gen_for_current.get().is_self_reply);
    let regenerating_msg_id = Memo::new(move |_| gen_for_current.get().regenerating_msg_id);

    let (more_menu_open, set_more_menu_open) = signal(false);
    let (revealed_len, set_revealed_len) = signal(0usize);

    let (editing_id, set_editing_id) = signal(None::<String>);
    let (edit_text, set_edit_text) = signal(String::new());
    let (open_prompt_id, set_open_prompt_id) = signal(None::<String>);

    // persists across streaming re-renders
    let (thought_open_states, set_thought_open_states) = signal(HashMap::<String, bool>::new());

    let message_list_ref = NodeRef::<Ul>::new();
    let (did_initial_scroll, set_did_initial_scroll) = signal(false);

    Effect::new(move |_| {
        let branch = active_branch.get();
        // this fires on mount before the chat has loaded, when active_branch
        // is still empty - waiting for a real branch keeps did_initial_scroll
        // from being spent on that empty pass, which used to leave nothing
        // to scroll to and never fire again once the real messages arrived
        if !did_initial_scroll.get_untracked() && !branch.is_empty() {
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
        // once the bytes have stopped, show the whole reply. the typewriter
        // used to still be a few characters behind, and that tail only
        // appeared when the saved message replaced the stream.
        let prefix: String = if !is_generating.get() || crate::api::get_text_speed() == 0 {
            full
        } else {
            full.chars().take(revealed_len.get()).collect()
        };
        let (body, thought) = crate::render::reasoning::extract_thinking(&prefix);
        // plot momentum notes are not part of the reply. strip them here so
        // a still-open block does not count as the visible text moving.
        let body = crate::render::safe_html::strip_plot_momentum(&body);
        (body, thought)
    });
    let has_thought = Memo::new(move |_| extracted_stream.get().1.is_some());

    // the cursor tracks visible text. a plot-momentum block keeps tokens
    // coming for several seconds without changing what is on screen, and
    // the bar used to sit under the last paragraph the whole time.
    let (show_cursor, set_show_cursor) = signal(false);
    let cursor_epoch = Rc::new(Cell::new(0u64));
    Effect::new(move |_| {
        let _visible = extracted_stream.get().0;
        if !is_generating.get() {
            set_show_cursor.set(false);
            return;
        }
        set_show_cursor.set(true);
        let epoch = cursor_epoch.get() + 1;
        cursor_epoch.set(epoch);
        let epochs = cursor_epoch.clone();
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(600).await;
            if epochs.get() == epoch {
                set_show_cursor.set(false);
            }
        });
    });

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
        // a full markdown rebuild every 12ms pegs the phone and the stream
        // read stops getting turns, so the end of the reply sits there.
        let interval_ms = (1000 / speed).max(80);
        let epoch_for_loop = generation_epoch.clone();
        let idle_pulls = Rc::new(Cell::new(0u32));

        leptos::task::spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(interval_ms).await;
                if !is_generating.get_untracked() || epoch_for_loop.get() != my_epoch {
                    break;
                }
                let total = streaming_reply.get_untracked().chars().count();
                let current = revealed_len.get_untracked();
                if current >= total {
                    // caught up. only spend a scroll if a layout clamp
                    // actually left them on the opening exchange. idling
                    // at the bottom shouldn't schedule a pin every tick
                    // for the whole reload.
                    let id_now = chat_id.get_untracked();
                    let follow = gen_states.with_untracked(|m| m.get(&id_now).is_some_and(|s| s.follow_tail));
                    if follow && crate::pages::chat_state::is_scroll_at_top() && !crate::pages::chat_state::is_scrolled_to_bottom() {
                        let n = idle_pulls.get();
                        idle_pulls.set(n.wrapping_add(1));
                        if n % 4 == 0 {
                            crate::pages::chat_state::scroll_to_bottom_soon();
                        }
                    } else if follow && !crate::pages::chat_state::is_scrolled_to_bottom() && !crate::pages::chat_state::is_scroll_at_top() {
                        gen_states.update(|m| {
                            if let Some(state) = m.get_mut(&id_now) {
                                state.follow_tail = false;
                            }
                        });
                    }
                    continue;
                }
                let behind = total - current;
                let step = super::chat_state::reveal_step(behind, speed);
                set_revealed_len.set((current + step).min(total));
                maintain_tail_follow(gen_states, &chat_id.get_untracked());
            }
        });
    });

    Effect::new(move |_| {
        install_tail_follow_break(gen_states, chat_id);
    });

    let signals = ChatSignals {
        chat_id,
        tree,
        set_tree,
        set_chat_meta,
        active_branch,
        draft,
        set_draft,
        set_pending_user_text,
        gen_states,
        outbox,
        set_more_menu_open,
        set_error,
        send_parent_id,
        set_send_parent_id,
        edit_text,
        set_editing_id,
        set_selected_children,
    };

    let fetch_tree = build_fetch_tree(signals);
    Effect::new({
        let fetch = fetch_tree.clone();
        move |_| fetch()
    });

    let send = build_send(signals, navigate.clone());
    let queued_lines = Memo::new(move |_| {
        let id = chat_id.get();
        outbox.with(|m| m.get(&id).cloned().unwrap_or_default())
    });
    let regenerate = build_regenerate(signals, navigate.clone());
    let continue_gen = build_continue_gen(signals, navigate.clone());
    let respond_as_me = build_respond_as_me(signals, navigate.clone());

    let delete = build_delete(signals, fetch_tree);
    let delete_all_below = build_delete_all_below(signals, fetch_tree);
    let toggle_visibility = build_toggle_visibility(signals, fetch_tree);
    let save_edit = build_save_edit(signals, fetch_tree);
    let select_sibling = build_select_sibling(signals);
    let select_branch_point = build_select_branch_point(signals);

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

        <div class="chat-header">
                <strong>{move || char_name_sig.get()}</strong>
                <div class="chat-header-tools">
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

                    let visible_msgs: Vec<_> = branch.into_iter().filter(|n| !n.deleted).collect();
                    let speaker_keys: Vec<String> = visible_msgs.iter().map(|m| {
                        if m.role == "user" {
                            "user".to_string()
                        } else {
                            m.character_id.clone().unwrap_or_default()
                        }
                    }).collect();
                    let last_idx = visible_msgs.len().saturating_sub(1);
                    // the optimistic tail is the end of the conversation until the saved
                    // rows replace it, so the message behind that tail is not "latest"
                    // untracked on purpose. reading these here rebuilt every
                    // row when a reply started or finished, and the phone
                    // clamped the scroller back to the opening exchange.
                    let tail_open = regenerating_msg_id.get_untracked().is_none()
                        && (is_generating.get_untracked()
                            || pending_user_text.get_untracked().is_some()
                            || !group_reply_log.get_untracked().is_empty());
                    visible_msgs
                        .into_iter()
                        .enumerate()
                        .map(move |(idx, message)| {
                            let grouped = idx > 0 && speaker_keys.get(idx) == speaker_keys.get(idx - 1);
                            let rhythm = if grouped { "grouped" } else { "turn-start" };
                            let latest = if idx == last_idx && !tail_open { "is-latest" } else { "" };
                            let id = message.id.clone();
                            let role = message.role.clone();
                            // saved text only. reading streaming_reply here
                            // subscribed the whole list, so every regenerate
                            // token destroyed every row and the scroller
                            // clamped back to the first exchange.
                            let content = message.content.clone();
                            let visible = message.visible;
                            let raw_prompt = message.raw_prompt.clone();
                            let has_stored_prompt = message.prompt_tokens.is_some() || raw_prompt.is_some();
                            let (speaker_name, speaker_avatar) =
                                speaker_for(&char_map, &fallback_character, &message.character_id);

                            let (_, thought) =
                                crate::render::reasoning::extract_thinking(&content);

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
                                <li
                                    tabindex="-1"
                                    on:click=focus_message_row
                                    class=format!(
                                    "message-row {} {} {} {} {}",
                                    role,
                                    if !visible { "dimmed" } else { "" },
                                    if is_selected_branch_point { "branch-selected" } else { "" },
                                    rhythm,
                                    latest,
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
                                            let thought_id = id.clone();
                                            let tos = thought_open_states;
                                            let stos = set_thought_open_states;
                                            view! {
                                                {thought.map(|t| view! { <Thought text=t message_id=thought_id.clone() open_states=tos set_open_states=stos forbid_media=forbid_media() /> })}
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
                                            let thought_id = id.clone();
                                            let row_id = id.clone();
                                            let saved = content.clone();
                                            let speaker = speaker_name.clone();
                                            let tos = thought_open_states;
                                            let stos = set_thought_open_states;
                                            // the stream subscription lives on this row only.
                                            // the list builder must not read streaming_reply,
                                            // or every token remounts the whole transcript.
                                            view! {
                                                {move || {
                                                    // is_generating drops when the bytes end, before
                                                    // the saved swipe is in the tree. this row has to
                                                    // keep the streamed text or it flashes the old reply.
                                                    let regen_here = regenerating_msg_id.get().as_deref() == Some(row_id.as_str());
                                                    let streamed = if regen_here { streaming_reply.get() } else { String::new() };
                                                    let live = regen_here && (is_generating.get() || !streamed.is_empty());
                                                    let content = if live { streamed } else { saved.clone() };
                                                    let (visible_body, thought) = crate::render::reasoning::extract_thinking(&content);
                                                    let rendered = crate::render::markdown::render_markdown(&visible_body, &speaker, &user_name_sig.get(), forbid_media());
                                                    view! {
                                                        {thought.map(|t| view! { <Thought text=t message_id=thought_id.clone() open_states=tos set_open_states=stos forbid_media=forbid_media() /> })}
                                                        <div>{rendered}</div>
                                                    }
                                                }}
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
                                                                            match crate::api::edit_message(&id, &content).await {
                                                                                Ok(()) => {
                                                                                    set_confirmed_greeting.set(idx);
                                                                                    fetch_tree();
                                                                                }
                                                                                Err(e) => set_error.set(Some(e)),
                                                                            }
                                                                        });
                                                                    } else {
                                                                        set_confirmed_greeting.set(idx);
                                                                    }
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
                                                                            set_selected_children.update(|map| {
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
                                            {has_stored_prompt.then(|| {
                                                let id_for_toggle = id.clone();
                                                let already_loaded = raw_prompt.is_some();
                                                view! {
                                                    <button
                                                        class="icon-btn view-prompt"
                                                        title="View the raw prompt sent for this message"
                                                        on:click=move |_| {
                                                            let toggle = {
                                                                let id_for_toggle = id_for_toggle.clone();
                                                                move || {
                                                                    set_open_prompt_id.update(|current| {
                                                                        *current = if current.as_deref() == Some(id_for_toggle.as_str()) {
                                                                            None
                                                                        } else {
                                                                            Some(id_for_toggle.clone())
                                                                        };
                                                                    });
                                                                }
                                                            };
                                                            if already_loaded {
                                                                toggle();
                                                                return;
                                                            }
                                                            let id_for_load = id_for_toggle.clone();
                                                            spawn_local(async move {
                                                                if let Ok(prompt) = crate::api::get_message_prompt(&id_for_load).await {
                                                                    set_tree.update(|t| {
                                                                        if let Some(node) = t.messages.get_mut(&id_for_load) {
                                                                            node.raw_prompt = prompt.raw_prompt;
                                                                            if prompt.prompt_tokens.is_some() {
                                                                                node.prompt_tokens = prompt.prompt_tokens;
                                                                            }
                                                                            if prompt.context_limit.is_some() {
                                                                                node.context_limit = prompt.context_limit;
                                                                            }
                                                                        }
                                                                    });
                                                                    set_open_prompt_id.set(Some(id_for_load));
                                                                }
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
                                                let is_gen = is_generating.get_untracked();
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
                <Show when=move || {
                    let live = gen_for_current.get();
                    regenerating_msg_id.get().is_none()
                        && (live.is_generating
                            || pending_user_text.get().is_some()
                            || !live.streaming_reply.is_empty()
                            || !live.group_reply_log.is_empty())
                }>
                    {move || {
                        pending_user_text.get().map(|text| {
                            let follows_user = active_branch.get().iter().rev().find(|n| !n.deleted).is_some_and(|n| n.role == "user");
                            let rhythm = if follows_user { "grouped" } else { "turn-start" };
                            view! {
                                <li class=format!("message-row user {rhythm}")>
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
                        let log = group_reply_log.get();
                        (0..log.len()).map(|i| {
                            let (character_id, name, content) = log[i].clone();
                            let grouped = i > 0 && log[i].0 == log[i - 1].0;
                            let rhythm = if grouped { "grouped" } else { "turn-start" };
                            let (_, avatar) = speaker_for(&char_map, &None, &Some(character_id));
                            let (visible_body, thought) = crate::render::reasoning::extract_thinking(&content);
                            let rendered = crate::render::markdown::render_markdown(&visible_body, &name, &user_name_sig.get(), forbid_media());
                            let thought_id = format!("group_{}", name);
                            let tos = thought_open_states;
                            let stos = set_thought_open_states;
                            view! {
                                <li class=format!("message-row assistant {rhythm}")>
                                    {avatar.filter(|url| !url.is_empty() && !forbid_media()).map(|url| view! {
                                        <div class="message-avatar-rect"><img src=url alt="avatar" /></div>
                                    })}
                                    <div class="message-content-wrapper">
                                        <div class="message-header"><strong>{name}</strong></div>
                                        <div class="message-body">
                                            {thought.map(|t| view! { <Thought text=t message_id=thought_id.clone() open_states=tos set_open_states=stos forbid_media=forbid_media() /> })}
                                            <div>{rendered}</div>
                                        </div>
                                    </div>
                                </li>
                            }
                        }).collect::<Vec<_>>().into_any()
                    }}
                    <Show when=move || {
                        let live = gen_for_current.get();
                        live.is_generating || !live.streaming_reply.is_empty()
                    }>
                    <li class=move || {
                        let self_reply = is_self_reply.get();
                        let log = group_reply_log.get();
                        let pending = pending_user_text.get().is_some();
                        let grouped = if let Some((cid, _, _)) = log.last() {
                            !self_reply && current_member.get().is_some_and(|(id, _)| id == *cid)
                        } else if self_reply {
                            pending || active_branch.get().iter().rev().find(|n| !n.deleted).is_some_and(|n| n.role == "user")
                        } else if pending {
                            false
                        } else {
                            let member = current_member.get().map(|(id, _)| id);
                            active_branch.get().iter().rev().find(|n| !n.deleted).is_some_and(|n| {
                                n.role != "user" && match member.as_deref() {
                                    Some(id) => n.character_id.as_deref() == Some(id),
                                    // 1:1 has one assistant, so a continued reply stays in the same turn
                                    None => tree.get().group.is_none(),
                                }
                            })
                        };
                        let role = if self_reply { "user" } else { "assistant" };
                        let rhythm = if grouped { "grouped" } else { "turn-start" };
                        format!("message-row {role} {rhythm}")
                    }>
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
                                <Thought
                                    forbid_media=forbid_media()
                                    text=Signal::derive(move || extracted_stream.get().1.unwrap_or_default())
                                    message_id="streaming"
                                    open_states=thought_open_states
                                    set_open_states=set_thought_open_states
                                />
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
                                {move || (is_generating.get() && show_cursor.get()).then(|| view! { <span class="streaming-cursor">"|"</span> })}
                            </div>
                        </div>
                    </li>
                    </Show>
                </Show>
            </ul>



            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}

            <form class="chat-input-bar" on:submit=send>
                <div class="chat-compose-stack">
                {move || {
                    let lines = queued_lines.get();
                    if lines.is_empty() {
                        ().into_any()
                    } else {
                        let rows = lines.into_iter().enumerate().map(move |(index, line)| {
                            view! {
                                <li class="chat-queue-item">
                                    <span class="chat-queue-text">{line}</span>
                                    <button
                                        type="button"
                                        class="chat-queue-remove"
                                        title="Remove"
                                        aria-label="Remove"
                                        on:click=move |_| signals.remove_outbox_at(&chat_id.get_untracked(), index)
                                    >
                                        "×"
                                    </button>
                                </li>
                            }
                        }).collect_view();
                        view! { <ul class="chat-queue">{rows}</ul> }.into_any()
                    }
                }}
                <div class="chat-composer">
                {move || {
                    if gen_for_current.get().blocks_send() {
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
                                                    "Continue"
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
                    class="chat-field"
                    type="text"
                    placeholder=Signal::derive(move || {
                        if gen_for_current.get().blocks_send() {
                            "Queue message"
                        } else {
                            "Message"
                        }
                    })
                    prop:value=draft
                    on:input=move |ev| set_draft.set(event_target_value(&ev))
                />
                <crate::components::mascot::Aeth
                    state=Signal::derive(move || {
                        if stream_error.get() { crate::components::mascot::MascotState::Error }
                        else if is_generating.get() { crate::components::mascot::MascotState::Thinking }
                        else { crate::components::mascot::MascotState::Idle }
                    })
                    class="mascot-chat-corner"
                />
                <button
                    class="primary send-btn"
                    type="submit"
                    title=Signal::derive(move || {
                        if gen_for_current.get().blocks_send() { "Queue" } else { "Send" }
                    })
                    disabled=Signal::derive(move || {
                        let draft_empty = draft.get().trim().is_empty();
                        let busy = gen_for_current.get().blocks_send();
                        let has_queue = !queued_lines.get().is_empty();
                        draft_empty && (busy || !has_queue)
                    })
                >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="22" y1="2" x2="11" y2="13"></line>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"></polygon>
                    </svg>
                </button>
                </div>
                </div>
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
