use leptos::prelude::*;

/// wraps an `<img>` whose `src` is a `/api/proxy?url=...` URL with a retry that
/// fires when the tab regains focus. a plain `<img>` never retries a failed
/// load on its own - on mobile, backgrounding the tab (or a dead cellular
/// connection) can silently kill an in-flight image fetch with the `error`
/// event only firing once the tab comes back (if it fires at all), leaving a
/// permanently broken image with no way to recover short of reloading the
/// whole page. this bumps a cache-busting query param and re-renders the
/// `src` whenever a currently-broken image's tab becomes visible again.
#[component]
pub fn ProxiedImage(
    #[prop(into)] src: String,
    #[prop(optional, into)] alt: String,
    #[prop(optional, into)] title: String,
    #[prop(optional, into)] style: String,
) -> impl IntoView {
    let (retry, set_retry) = signal(0u32);
    let (errored, set_errored) = signal(false);

    let full_src = move || {
        let r = retry.get();
        if r == 0 { src.clone() } else { format!("{src}&retry={r}") }
    };

    let handle = window_event_listener_untyped("visibilitychange", move |_| {
        let visible = web_sys::window()
            .and_then(|w| w.document())
            .map(|d| d.visibility_state() == web_sys::VisibilityState::Visible)
            .unwrap_or(false);
        if visible && errored.get_untracked() {
            set_errored.set(false);
            set_retry.update(|r| *r += 1);
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        <img
            src=full_src
            alt=alt
            title=title
            style=style
            on:error=move |_| set_errored.set(true)
            on:load=move |_| set_errored.set(false)
        />
    }
}
