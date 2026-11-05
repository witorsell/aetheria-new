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
    /// the reply finished streaming and the saved row is not on screen yet.
    /// a send in this window queues, so it does not parent onto the turn
    /// before the reply that is still being saved.
    pub tail_hold: bool,
    pub streaming_reply: String,
    pub current_member: Option<(String, String)>,
    pub group_reply_log: Vec<(String, String, String)>,
    pub is_self_reply: bool,
    pub stream_error: bool,
    pub regenerating_msg_id: Option<String>,
    /// the user was reading the tail when this reply started. a layout clamp
    /// that jumps `.main-content` to scroll 0 is not them leaving, so this
    /// stays set until they actually scroll into the middle of the history.
    pub follow_tail: bool,
}

impl GenState {
    /// true while a reply owns the composer. a typed line queues instead of
    /// starting a second generation that would parent on the wrong turn.
    pub fn blocks_send(&self) -> bool {
        self.is_generating || self.tail_hold
    }
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
    pub set_chat_meta: WriteSignal<Option<crate::api::Chat>>,
    pub active_branch: Memo<Vec<crate::api::MessageNode>>,
    pub draft: ReadSignal<String>,
    pub set_draft: WriteSignal<String>,
    pub set_pending_user_text: WriteSignal<Option<String>>,
    pub gen_states: RwSignal<HashMap<String, GenState>>,
    /// lines typed while a reply was still running, keyed by chat id.
    /// memory only: a refresh drops them. not cleared with the live tail.
    pub outbox: RwSignal<HashMap<String, Vec<String>>>,
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

    /// swap the tree in, dropping swipe selections that point at a message
    /// the new tree does not contain. those selections used to end the walk
    /// early, so everything after the opening exchange stopped rendering.
    pub fn apply_tree(&self, tree: crate::api::MessageTree) {
        self.set_selected_children.update(|map| {
            map.retain(|parent, child| {
                tree.messages
                    .get(parent)
                    .is_some_and(|node| node.children.iter().any(|id| id == child))
            });
        });
        self.set_tree.set(tree);
    }

    /// drop the optimistic tail (the typed message and the streamed reply)
    /// once the saved tree is on screen. clearing it earlier leaves a hole
    /// until the reload arrives.
    pub fn clear_live_tail(&self, id: &str) {
        self.update_gen(id, |s| {
            s.is_generating = false;
            s.streaming_reply.clear();
            s.current_member = None;
            s.group_reply_log.clear();
            s.regenerating_msg_id = None;
            s.is_self_reply = false;
        });
        self.set_pending_user_text.set(None);
    }

    pub fn enqueue_outbox(&self, id: &str, raw: &str) -> bool {
        let mut pushed = false;
        self.outbox.update(|m| {
            let queue = m.entry(id.to_string()).or_default();
            pushed = enqueue_line(queue, raw);
            if queue.is_empty() {
                m.remove(id);
            }
        });
        pushed
    }

    pub fn pop_outbox(&self, id: &str) -> Option<String> {
        let mut next = None;
        self.outbox.update(|m| {
            if let Some(queue) = m.get_mut(id) {
                next = pop_front_line(queue);
                if queue.is_empty() {
                    m.remove(id);
                }
            }
        });
        next
    }

    /// put a line back ahead of anything queued behind it. used when a send
    /// bailed before the request, so the line is not lost and does not jump
    /// the queue.
    pub fn push_front_outbox(&self, id: &str, raw: &str) {
        self.outbox.update(|m| {
            let queue = m.entry(id.to_string()).or_default();
            push_front_line(queue, raw);
            if queue.is_empty() {
                m.remove(id);
            }
        });
    }

    pub fn remove_outbox_at(&self, id: &str, index: usize) {
        self.outbox.update(|m| {
            if let Some(queue) = m.get_mut(id) {
                remove_line_at(queue, index);
                if queue.is_empty() {
                    m.remove(id);
                }
            }
        });
    }
}

fn enqueue_line(queue: &mut Vec<String>, raw: &str) -> bool {
    let line = raw.trim();
    if line.is_empty() {
        return false;
    }
    queue.push(line.to_string());
    true
}

fn pop_front_line(queue: &mut Vec<String>) -> Option<String> {
    if queue.is_empty() {
        None
    } else {
        Some(queue.remove(0))
    }
}

fn push_front_line(queue: &mut Vec<String>, raw: &str) {
    let line = raw.trim();
    if line.is_empty() {
        return;
    }
    queue.insert(0, line.to_string());
}

fn remove_line_at(queue: &mut Vec<String>, index: usize) -> bool {
    if index >= queue.len() {
        return false;
    }
    queue.remove(index);
    true
}

fn main_content() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.query_selector(".main-content").ok().flatten()
}

fn overflows(el: &web_sys::Element) -> bool {
    el.scroll_height() > el.client_height() + 1
}

/// `.main-content` is the scroller on a locked chat page. on a phone the
/// document itself still scrolls when the shell spills out of the viewport,
/// and pinning only `.main-content` then does nothing.
fn overflowing_scrollers() -> Vec<web_sys::Element> {
    let mut out = Vec::new();
    if let Some(el) = main_content() {
        if overflows(&el) {
            out.push(el);
        }
    }
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.scrolling_element() {
            if overflows(&el) {
                out.push(el);
            }
        }
    }
    out
}

/// true when every real scroller is sitting on the first row. replacing the
/// streaming bubble drops `scrollHeight` for a frame and the browser clamps
/// `scrollTop` to 0, which looks the same as a jump to the opening exchange.
pub(super) fn is_scroll_at_top() -> bool {
    let scrollers = overflowing_scrollers();
    scrollers.is_empty() || scrollers.iter().all(|el| el.scroll_top() < 1)
}

/// true when every overflowing scroller is at (or very near) the bottom.
/// being clamped to the top does NOT count. a user who scrolled up to read
/// history should keep their position.
pub(super) fn is_scrolled_to_bottom() -> bool {
    let scrollers = overflowing_scrollers();
    if scrollers.is_empty() {
        return true;
    }
    scrollers.iter().all(|el| el.scroll_height() - (el.scroll_top() + el.client_height()) < 150)
}

/// `follow` stays on through a clamp to the top. it turns off only when the
/// viewport is actually parked in the middle of the history.
pub(super) fn keep_following(follow: bool, at_bottom: bool, at_top: bool) -> bool {
    follow && (at_bottom || at_top)
}

/// characters to paint on this tick. the bubble is rebuilt from scratch
/// every tick, and a one-character crawl once the model is ahead is what
/// left the last line sitting there. a short lead keeps the text-speed
/// pace. anything longer than a beat is painted in one go.
pub(super) fn reveal_step(behind: usize, speed: u32) -> usize {
    if behind == 0 || speed == 0 {
        return behind;
    }
    let pace = (speed as usize / 10).max(1);
    if behind > pace.saturating_mul(3) {
        behind
    } else {
        pace.min(behind)
    }
}

fn scroll_main_to_bottom() {
    for el in overflowing_scrollers() {
        el.set_scroll_top(el.scroll_height());
    }
    // a scroller that does not overflow yet still needs the pin. the next
    // frame is when the new rows make it overflow.
    if let Some(el) = main_content() {
        el.set_scroll_top(el.scroll_height());
    }
}

/// scrolls `.main-content` to the bottom once layout has had a frame to
/// catch the node we just replaced, then again in case that first pass ran
/// before the new height existed.
pub(super) fn scroll_to_bottom_soon() {
    leptos::task::spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(16).await;
        scroll_main_to_bottom();
        gloo_timers::future::TimeoutFuture::new(64).await;
        scroll_main_to_bottom();
    });
}

/// same pin as [`scroll_to_bottom_soon`], plus a later pass for the full
/// list rebuild that lands when a reply is saved. markdown for the whole
/// branch is taller than one frame.
pub(super) fn scroll_to_bottom_settled() {
    leptos::task::spawn_local(async move {
        for delay in [16u32, 80, 240] {
            gloo_timers::future::TimeoutFuture::new(delay).await;
            scroll_main_to_bottom();
        }
    });
}

/// while a reply is revealing, pull the tail back into view if we're still
/// following it. a clamp to scroll 0 keeps following. a viewport parked
/// between the ends means the user left, and we stop pulling.
pub(super) fn maintain_tail_follow(gen_states: RwSignal<HashMap<String, GenState>>, chat_id: &str) {
    let follow = gen_states.with_untracked(|m| m.get(chat_id).is_some_and(|s| s.follow_tail));
    let at_bottom = is_scrolled_to_bottom();
    let at_top = is_scroll_at_top();
    if !keep_following(follow, at_bottom, at_top) {
        if follow {
            gen_states.update(|m| {
                if let Some(state) = m.get_mut(chat_id) {
                    state.follow_tail = false;
                }
            });
        }
        return;
    }
    scroll_to_bottom_soon();
}

/// a wheel-up or a finger drag toward the start of the chat is the user
/// leaving the tail. layout clamps don't produce these events, so they
/// can't be mistaken for that.
pub(super) fn install_tail_follow_break(gen_states: RwSignal<HashMap<String, GenState>>, chat_id: Memo<String>) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let release = std::rc::Rc::new(move || {
        let id = chat_id.get_untracked();
        let following = gen_states.with_untracked(|m| m.get(&id).is_some_and(|s| s.follow_tail));
        if !following {
            return;
        }
        gen_states.update(|m| {
            if let Some(state) = m.get_mut(&id) {
                state.follow_tail = false;
            }
        });
    });

    let release_wheel = release.clone();
    let wheel = Closure::<dyn FnMut(web_sys::WheelEvent)>::new(move |ev: web_sys::WheelEvent| {
        if ev.delta_y() < -4.0 && event_in_scroller(&ev) {
            (*release_wheel)();
        }
    });
    let _ = doc.add_event_listener_with_callback("wheel", wheel.as_ref().unchecked_ref());
    wheel.forget();

    let last_y = std::rc::Rc::new(std::cell::Cell::new(f64::NAN));
    let start_y = last_y.clone();
    let touch_start = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
        if let Some(touch) = ev.touches().item(0) {
            start_y.set(touch.client_y() as f64);
        }
    });
    let _ = doc.add_event_listener_with_callback("touchstart", touch_start.as_ref().unchecked_ref());
    touch_start.forget();

    let move_y = last_y;
    let release_touch = release;
    let touch_move = Closure::<dyn FnMut(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
        if !event_in_scroller(&ev) {
            return;
        }
        let Some(touch) = ev.touches().item(0) else { return };
        let y = touch.client_y() as f64;
        let prev = move_y.get();
        move_y.set(y);
        // finger moving down scrolls the transcript toward the opening lines
        if prev.is_finite() && y - prev > 16.0 {
            (*release_touch)();
        }
    });
    let _ = doc.add_event_listener_with_callback("touchmove", touch_move.as_ref().unchecked_ref());
    touch_move.forget();
}

fn event_in_scroller(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    let Some(target) = ev.target() else { return false };
    let Ok(el) = target.dyn_into::<web_sys::Element>() else { return false };
    el.closest(".main-content").ok().flatten().is_some()
}

#[cfg(test)]
mod tests {
    use super::{enqueue_line, keep_following, pop_front_line, push_front_line, remove_line_at, reveal_step};

    #[test]
    fn a_clamp_to_the_top_does_not_count_as_leaving_the_tail() {
        assert!(keep_following(true, false, true));
    }

    #[test]
    fn reading_the_middle_lets_go_of_the_tail() {
        assert!(!keep_following(true, false, false));
    }

    #[test]
    fn staying_at_the_bottom_keeps_the_tail() {
        assert!(keep_following(true, true, false));
    }

    #[test]
    fn an_already_released_tail_stays_released() {
        assert!(!keep_following(false, false, true));
        assert!(!keep_following(false, true, false));
    }

    #[test]
    fn a_short_lead_stays_on_the_text_speed_pace() {
        assert_eq!(reveal_step(0, 40), 0);
        assert_eq!(reveal_step(5, 40), 4);
        assert_eq!(reveal_step(3, 40), 3);
    }

    #[test]
    fn a_backlog_is_painted_in_one_step() {
        assert_eq!(reveal_step(80, 40), 80);
        assert_eq!(reveal_step(400, 20), 400);
    }

    #[test]
    fn disabled_text_speed_shows_the_whole_buffer() {
        assert_eq!(reveal_step(80, 0), 80);
    }

    #[test]
    fn outbox_keeps_order_and_drops_blank_lines() {
        let mut queue = Vec::new();
        assert!(!enqueue_line(&mut queue, "   "));
        assert!(enqueue_line(&mut queue, " first "));
        assert!(enqueue_line(&mut queue, "second"));
        assert_eq!(queue, vec!["first".to_string(), "second".to_string()]);
        assert_eq!(pop_front_line(&mut queue).as_deref(), Some("first"));
        assert_eq!(pop_front_line(&mut queue).as_deref(), Some("second"));
        assert!(pop_front_line(&mut queue).is_none());
    }

    #[test]
    fn outbox_remove_drops_only_that_line() {
        let mut queue = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(remove_line_at(&mut queue, 1));
        assert_eq!(queue, vec!["a".to_string(), "c".to_string()]);
        assert!(!remove_line_at(&mut queue, 4));
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn outbox_push_front_restores_a_failed_send_ahead_of_the_rest() {
        let mut queue = Vec::new();
        assert!(enqueue_line(&mut queue, "later"));
        push_front_line(&mut queue, " first ");
        push_front_line(&mut queue, "  ");
        assert_eq!(pop_front_line(&mut queue).as_deref(), Some("first"));
        assert_eq!(pop_front_line(&mut queue).as_deref(), Some("later"));
    }
}
