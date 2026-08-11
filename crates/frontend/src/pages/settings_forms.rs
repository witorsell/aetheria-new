use leptos::prelude::*;
use leptos::task::spawn_local;

pub(super) const SPEED_PRESETS: [(u32, &str); 4] = [(0, "Disabled"), (20, "Slow"), (40, "Normal"), (80, "Fast")];

pub(super) fn speed_to_preset_index(speed: u32) -> usize {
    SPEED_PRESETS
        .iter()
        .enumerate()
        .min_by_key(|(_, (value, _))| (*value as i64 - speed as i64).abs())
        .map(|(index, _)| index)
        .unwrap_or(2)
}

#[component]
pub(super) fn UserProfileForm(
    display_name: Signal<String>,
    set_display_name: WriteSignal<String>,
    persona: Signal<String>,
    set_persona: WriteSignal<String>,
    use_persona: Signal<bool>,
    set_use_persona: WriteSignal<bool>,
    avatar_url: Signal<Option<String>>,
    set_avatar_url: WriteSignal<Option<String>>,
    user_error: Signal<Option<String>>,
    set_user_error: WriteSignal<Option<String>>,
    user_saved: Signal<bool>,
    set_user_saved: WriteSignal<bool>,
    forbid_external_media: Signal<bool>,
) -> impl IntoView {
    let on_user_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_user_saved.set(false);
        let name = display_name.get_untracked();
        let name_arg = if name.trim().is_empty() { None } else { Some(name) };
        let pers = persona.get_untracked();
        let pers_arg = if pers.trim().is_empty() { None } else { Some(pers) };

        spawn_local(async move {
            let req = crate::api::UpdateMeRequest {
                display_name: name_arg,
                persona: pers_arg,
                use_persona: use_persona.get_untracked(),
            };
            match crate::api::update_me(req).await {
                Ok(()) => {
                    set_user_error.set(None);
                    set_user_saved.set(true);
                }
                Err(e) => set_user_error.set(Some(e)),
            }
        });
    };

    view! {
        <form class="settings-form" on:submit=on_user_submit style="display: flex; flex-direction: column; gap: 2.5rem; margin-bottom: 5rem;">
            <div class="field" style="margin: 0;">
                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Display Name"</label>
                <input
                    type="text"
                    placeholder="Your name in roleplays..."
                    prop:value=display_name
                    on:input=move |ev| set_display_name.set(event_target_value(&ev))
                    style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                />
            </div>

            <div class="field" style="margin: 0;">
                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Avatar"</label>
                <div style="display: flex; align-items: center; gap: 1rem;">
                    <Show when=move || avatar_url.get().is_some()>
                        <div class="avatar-preview">
                            {move || {
                                if !forbid_external_media.get() {
                                    view! { <img src=move || avatar_url.get().unwrap_or_default() alt="Avatar preview" /> }.into_any()
                                } else {
                                    view! { <div>"Media Forbidden"</div> }.into_any()
                                }
                            }}
                        </div>
                    </Show>
                    <input type="file" accept="image/*"
                        on:change={
                            let set_avatar_url = set_avatar_url.clone();
                            let set_user_error = set_user_error.clone();
                            move |ev| {
                                let target = event_target::<web_sys::HtmlInputElement>(&ev);
                                if let Some(files) = target.files() {
                                    if let Some(file) = files.get(0) {
                                        let form_data = web_sys::FormData::new().unwrap();
                                        form_data.append_with_blob("avatar", &file).unwrap();
                                        let set_avatar_url = set_avatar_url.clone();
                                        let set_user_error = set_user_error.clone();
                                        spawn_local(async move {
                                            let req = web_sys::RequestInit::new();
                                            req.set_method("POST");
                                            req.set_body(&form_data);
                                            let request = web_sys::Request::new_with_str_and_init("/api/me/avatar", &req).unwrap();
                                            let window = web_sys::window().unwrap();
                                            match wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await {
                                                Ok(resp_val) => {
                                                    use wasm_bindgen::JsCast;
                                                    let resp: web_sys::Response = resp_val.dyn_into().unwrap();
                                                    if resp.ok() {
                                                        if let Ok(json_val) = wasm_bindgen_futures::JsFuture::from(resp.json().unwrap()).await {
                                                            if let Ok(url_val) = js_sys::Reflect::get(&json_val, &wasm_bindgen::JsValue::from_str("url")) {
                                                                if let Some(url_str) = url_val.as_string() {
                                                                    set_avatar_url.set(Some(url_str));
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        set_user_error.set(Some("Failed to upload avatar".to_string()));
                                                    }
                                                }
                                                Err(_) => set_user_error.set(Some("Network error".to_string())),
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    />
                </div>
            </div>

            <div class="field" style="margin: 0;">
                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Persona"</label>
                <textarea
                    placeholder="Describe your persona..."
                    prop:value=persona
                    on:input=move |ev| set_persona.set(event_target_value(&ev))
                    style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); color: #fff; font-family: var(--font-body); padding: 1rem; font-size: 0.95rem; outline: none; width: 100%; min-height: 120px; resize: vertical; line-height: 1.5;"
                ></textarea>

                <label class="checkbox-field" style="display: flex; align-items: center; gap: 0.5rem; margin-top: 1rem; color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; cursor: pointer;">
                    <input
                        type="checkbox"
                        prop:checked=use_persona
                        on:change=move |ev| set_use_persona.set(event_target_checked(&ev))
                        style="accent-color: #fff;"
                    />
                    "ENABLE PERSONA INJECTION"
                </label>
            </div>

            <div style="display: flex; align-items: center; justify-content: flex-end; gap: 1.5rem; border-top: 1px solid var(--color-border); padding-top: 2rem;">
                {move || {
                    user_error.get().map(|err| {
                        view! { <span style="color: #ff4444; font-family: monospace; font-size: 0.85rem; text-transform: uppercase;">"ERROR: "{err}</span> }
                    })
                }}
                {move || {
                    (user_saved.get() && user_error.get().is_none()).then(|| {
                        view! { <span style="color: #44ff44; font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.1em;">"PROFILE SAVED."</span> }
                    })
                }}

                <button
                    type="submit"
                    style="background: #fff; color: #000; border: none; padding: 0.75rem 2rem; font-family: monospace; text-transform: uppercase; font-size: 0.9rem; font-weight: 600; cursor: pointer; letter-spacing: 0.1em; transition: opacity 0.2s;"
                    on:mouseover=|_| {}
                >
                    "SAVE PROFILE"
                </button>
            </div>
        </form>
    }
}
