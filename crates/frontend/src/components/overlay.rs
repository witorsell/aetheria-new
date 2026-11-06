use leptos::prelude::*;
use wasm_bindgen::JsCast;

pub fn overlay_keyboard(open: Signal<bool>, close: Callback<()>, selector: &'static str) {
    let previous = std::rc::Rc::new(std::cell::RefCell::new(None::<web_sys::HtmlElement>));
    let saved = previous.clone();
    Effect::new(move |_| {
        let document = document();
        if open.get() {
            if saved.borrow().is_none() {
                *saved.borrow_mut() = document.active_element().and_then(|e| e.dyn_into().ok());
            }
            leptos::task::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(0).await;
                if open.get_untracked() {
                    if let Ok(Some(panel)) = document.query_selector(selector) {
                        if let Ok(Some(first)) = panel.query_selector("button, [href], input, select, textarea") {
                            if let Some(first) = first.dyn_ref::<web_sys::HtmlElement>() { let _ = first.focus(); }
                        }
                    }
                }
            });
        } else if let Some(element) = saved.borrow_mut().take() {
            let _ = element.focus();
        }
    });
    let handle = window_event_listener(leptos::ev::keydown, move |event| {
        if !open.get_untracked() { return; }
        if event.key() == "Escape" {
            event.prevent_default();
            close.run(());
        } else if event.key() == "Tab" {
            let document = document();
            let Ok(Some(panel)) = document.query_selector(selector) else { return; };
            let Ok(nodes) = panel.query_selector_all("button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex=\"0\"]") else { return; };
            let items: Vec<web_sys::HtmlElement> = (0..nodes.length()).filter_map(|i| nodes.item(i))
                .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
                .filter(|e| e.closest("[hidden]").ok().flatten().is_none()).collect();
            if let (Some(first), Some(last)) = (items.first(), items.last()) {
                let active = document.active_element();
                let target = if event.shift_key() && active.as_ref() == Some(first.as_ref()) { Some(last) }
                    else if !event.shift_key() && active.as_ref() == Some(last.as_ref()) { Some(first) } else { None };
                if let Some(target) = target { event.prevent_default(); let _ = target.focus(); }
            }
        }
    });
    let previous = send_wrapper::SendWrapper::new(previous);
    on_cleanup(move || {
        handle.remove();
        if let Some(element) = previous.borrow_mut().take() { let _ = element.focus(); }
    });
}
