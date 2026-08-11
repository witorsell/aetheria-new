use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (confirm, set_confirm) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (mode, set_mode) = signal("login");
    let (reg_enabled, set_reg_enabled) = signal(false);

    spawn_local(async move {
        let enabled = crate::api::registration_enabled().await;
        set_reg_enabled.set(enabled);
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let m = mode.get_untracked();
        let u = username.get_untracked();
        let p = password.get_untracked();
        spawn_local(async move {
            if m == "register" {
                let c = confirm.get_untracked();
                if p != c {
                    set_error.set(Some("passwords don't match".into()));
                    return;
                }
                if p.len() < 4 {
                    set_error.set(Some("password must be at least 4 characters".into()));
                    return;
                }
                match crate::api::register(&u, &p).await {
                    Ok(()) => {
                        let _ = web_sys::window().unwrap().location().set_href("/");
                    }
                    Err(e) => set_error.set(Some(e)),
                }
            } else {
                match crate::api::login(&u, &p).await {
                    Ok(()) => {
                        let _ = web_sys::window().unwrap().location().set_href("/");
                    }
                    Err(e) => set_error.set(Some(e)),
                }
            }
        });
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <form on:submit=on_submit>
                    <h1>"aetheria"</h1>
                    <input
                        type="text"
                        placeholder="Username"
                        prop:value=username
                        on:input=move |ev| set_username.set(event_target_value(&ev))
                    />
                    <input
                        type="password"
                        placeholder="Password"
                        prop:value=password
                        on:input=move |ev| set_password.set(event_target_value(&ev))
                    />
                    {move || if mode.get() == "register" {
                        view! {
                            <input
                                type="password"
                                placeholder="Confirm password"
                                prop:value=confirm
                                on:input=move |ev| set_confirm.set(event_target_value(&ev))
                            />
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    <button type="submit" class="primary">
                        {move || if mode.get() == "register" { "Create account" } else { "Log in" }}
                    </button>
                    {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                </form>
            </div>
            {move || if reg_enabled.get() && mode.get() == "login" {
                view! {
                    <p style="margin-top: 1.5rem; font-family: monospace; font-size: 0.75rem; color: var(--color-text-muted);">
                        <a
                            href="javascript:void(0)"
                            on:click=move |_| set_mode.set("register")
                            style="color: inherit; text-decoration: none; border-bottom: 1px dotted var(--color-border);"
                        >"Create an account"</a>
                    </p>
                }.into_any()
            } else if reg_enabled.get() && mode.get() == "register" {
                view! {
                    <p style="margin-top: 1.5rem; font-family: monospace; font-size: 0.75rem; color: var(--color-text-muted);">
                        <a
                            href="javascript:void(0)"
                            on:click=move |_| set_mode.set("login")
                            style="color: inherit; text-decoration: none; border-bottom: 1px dotted var(--color-border);"
                        >"Already have an account? Log in"</a>
                    </p>
                }.into_any()
            } else {
                ().into_any()
            }}
            <div style="margin-top: 1.5rem; display: flex; gap: 1rem; justify-content: center; font-family: monospace; font-size: 0.75rem; letter-spacing: 0.05em; color: var(--color-text-muted);">
                <a href="/terms" style="color: inherit; text-decoration: none; border-bottom: 1px dotted var(--color-border);">"Terms of Service"</a>
                <a href="/privacy" style="color: inherit; text-decoration: none; border-bottom: 1px dotted var(--color-border);">"Privacy Policy"</a>
            </div>
        </div>
    }
}
