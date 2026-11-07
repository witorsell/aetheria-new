use leptos::prelude::*;

/// wraps an `<img>` with a retry that fires when the tab regains focus. a plain `<img>` never retries a failed
/// load on its own - on mobile, backgrounding the tab (or a dead cellular
/// connection) can silently kill an in-flight image fetch with the `error`
/// event only firing once the tab comes back (if it fires at all), leaving a
/// permanently broken image with no way to recover short of reloading the
/// whole page. this bumps a cache-busting query param and re-renders the
/// `src` whenever a currently-broken image's tab becomes visible again.
/// adds the cache-busting retry param, picking `?` or `&` depending on
/// whether the url already carries a query string
fn with_retry(src: &str, retry: u32) -> String {
    if retry == 0 {
        return src.to_string();
    }
    let join = if src.contains('?') { '&' } else { '?' };
    format!("{src}{join}retry={retry}")
}

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
        with_retry(&src, retry.get())
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
            referrerpolicy="no-referrer"
            on:error=move |_| set_errored.set(true)
            on:load=move |_| set_errored.set(false)
        />
    }
}

#[cfg(test)]
mod tests {
    use super::with_retry;

    #[test]
    fn first_load_keeps_the_url_as_is() {
        assert_eq!(with_retry("https://x.test/a.png", 0), "https://x.test/a.png");
    }

    #[test]
    fn retry_picks_the_right_separator() {
        assert_eq!(with_retry("https://x.test/a.png", 2), "https://x.test/a.png?retry=2");
        assert_eq!(with_retry("https://x.test/a.png?v=1", 2), "https://x.test/a.png?v=1&retry=2");
    }
}
