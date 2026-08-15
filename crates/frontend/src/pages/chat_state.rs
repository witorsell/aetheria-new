use leptos::prelude::*;
use std::collections::HashMap;

/// per-chat generation state, keyed by chat id in [`ChatSignals::gen_states`]
/// rather than held as flat page-level signals. `ChatPage` is a single
/// persistent component instance reused across `/chat/A` -> `/chat/B`
/// navigation (routed by a `Memo<String>` chat_id, not a remount), so a flat
/// `is_generating`/`streaming_reply` used to bleed a stream started in one
/// chat into whatever chat was on screen when it finished - see the fix that
/// introduced this struct for the full writeup. Keying by chat id lets a
/// background stream keep running correctly for the chat it belongs to while
/// the view for a different chat stays untouched.
#[derive(Clone, Default, PartialEq)]
pub struct GenState {
    pub is_generating: bool,
    pub streaming_reply: String,
    pub current_member: Option<(String, String)>,
    pub group_reply_log: Vec<(String, String, String)>,
    pub is_self_reply: bool,
    pub stream_error: bool,
    pub regenerating_msg_id: Option<String>,
}

/// the reactive state generation handlers ([`super::chat_generation`]) and
/// per-message actions ([`super::chat_actions`]) both need. bundled so those
/// modules take one param instead of twenty, not because the fields belong
/// to a single concept - every field here is a `Copy` signal handle, so the
/// struct is Copy too and cheap to pass into every closure that needs it.
#[derive(Clone, Copy)]
pub struct ChatSignals {
    pub chat_id: Memo<String>,
    pub tree: ReadSignal<crate::api::MessageTree>,
    pub set_tree: WriteSignal<crate::api::MessageTree>,
    pub active_branch: Memo<Vec<crate::api::MessageNode>>,
    pub draft: ReadSignal<String>,
    pub set_draft: WriteSignal<String>,
    pub set_pending_user_text: WriteSignal<Option<String>>,
    pub gen_states: RwSignal<HashMap<String, GenState>>,
    pub set_more_menu_open: WriteSignal<bool>,
    pub set_error: WriteSignal<Option<String>>,
    pub send_parent_id: ReadSignal<Option<String>>,
    pub set_send_parent_id: WriteSignal<Option<String>>,
    pub edit_text: ReadSignal<String>,
    pub set_editing_id: WriteSignal<Option<String>>,
    pub set_selected_children: WriteSignal<HashMap<String, String>>,
}

impl ChatSignals {
    /// snapshot of the given chat's generation state, `GenState::default()`
    /// (not generating, empty) if that chat has no entry yet.
    pub fn gen_for(&self, id: &str) -> GenState {
        self.gen_states.with(|m| m.get(id).cloned().unwrap_or_default())
    }

    /// mutate the given chat's generation state in place, creating a default
    /// entry first if this is that chat's first generation.
    pub fn update_gen(&self, id: &str, f: impl FnOnce(&mut GenState)) {
        self.gen_states.update(|m| f(m.entry(id.to_string()).or_default()));
    }
}

/// true when `.main-content` is scrolled at (or near) the bottom. used to
/// decide whether an incoming stream delta should auto-scroll the view -
/// also treats "scrolled to the very top" as near-bottom, since a delta on
/// the very first render (before layout has any scroll range yet) should
/// still pin to the bottom rather than leave it wherever it happened to be.
pub(super) fn is_near_bottom() -> bool {
    let Some(window) = web_sys::window() else { return false };
    let Some(doc) = window.document() else { return false };
    let Ok(Some(el)) = doc.query_selector(".main-content") else { return false };
    let scroll_top = el.scroll_top();
    let client_height = el.client_height();
    let scroll_height = el.scroll_height();
    scroll_height - (scroll_top + client_height) < 150 || scroll_top < 1
}

/// true when `.main-content` is scrolled to (or very near) the bottom.
/// unlike [`is_near_bottom`], being scrolled to the top does NOT count -
/// used before a tree reload or sibling switch, where a user who scrolled
/// up to read history should keep their position rather than get pulled
/// down just because the top happens to be in view.
pub(super) fn is_scrolled_to_bottom() -> bool {
    let Some(window) = web_sys::window() else { return false };
    let Some(doc) = window.document() else { return false };
    let Ok(Some(el)) = doc.query_selector(".main-content") else { return false };
    let scroll_top = el.scroll_top();
    let client_height = el.client_height();
    let scroll_height = el.scroll_height();
    scroll_height - (scroll_top + client_height) < 150
}

/// scrolls `.main-content` to the bottom shortly after the DOM has had a
/// chance to grow to fit whatever content just arrived.
pub(super) fn scroll_to_bottom_soon() {
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
