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

        spawn_local(async move {
            let req = crate::api::UpdateMeRequest {
                display_name: name_arg,
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

#[component]
pub(super) fn PersonaManager() -> impl IntoView {
    let personas = LocalResource::new(|| async move { crate::api::list_personas().await });
    let me = LocalResource::new(|| async move { crate::api::fetch_me().await });

    let (new_name, set_new_name) = signal(String::new());
    let (new_description, set_new_description) = signal(String::new());
    let (error, set_error) = signal(String::new());
    let (creating, set_creating) = signal(false);
    let (delete_target, set_delete_target) = signal(Option::<(String, String)>::None);
    let (deleting, set_deleting) = signal(false);

    let create = move |_| {
        if creating.get_untracked() {
            return;
        }
        let name = new_name.get_untracked();
        if name.trim().is_empty() {
            set_error.set("Name cannot be empty".to_string());
            return;
        }
        set_creating.set(true);
        let description = new_description.get_untracked();
        spawn_local(async move {
            let input = crate::api::PersonaInput {
                name: &name,
                description: if description.trim().is_empty() { None } else { Some(&description) },
            };
            match crate::api::create_persona(input).await {
                Ok(_) => {
                    set_new_name.set(String::new());
                    set_new_description.set(String::new());
                    set_error.set(String::new());
                    personas.refetch();
                }
                Err(e) => set_error.set(e),
            }
            set_creating.set(false);
        });
    };

    let confirm_delete = move |_: leptos::ev::MouseEvent| {
        let Some((id, _)) = delete_target.get_untracked() else { return };
        set_deleting.set(true);
        spawn_local(async move {
            match crate::api::delete_persona(&id).await {
                Ok(()) => {
                    set_error.set(String::new());
                    personas.refetch();
                    me.refetch();
                    set_delete_target.set(None);
                }
                Err(e) => set_error.set(e),
            }
            set_deleting.set(false);
        });
    };

    view! {
        <div>
            <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Personas"</label>

            <Suspense fallback=move || view! { <div>"Loading personas..."</div> }>
                {move || {
                    let list = personas.get().and_then(|r| r.ok()).unwrap_or_default();
                    let active_id = me.get().and_then(|r| r.ok()).and_then(|m| m.active_persona_id);
                    view! {
                        <div style="display: flex; flex-direction: column; gap: 0.75rem;">
                            {list.into_iter().map(|p| {
                                let is_active = active_id.as_deref() == Some(p.id.as_str());
                                let id_for_activate = p.id.clone();
                                let id_for_delete = p.id.clone();
                                let name_for_delete = p.name.clone();
                                view! {
                                    <div style="display: flex; align-items: center; gap: 1rem; padding: 0.75rem; border: 1px solid var(--color-border); background: rgba(0,0,0,0.15); flex-wrap: wrap;">
                                        {p.avatar_url.clone().map(|url| view! {
                                            <img src=url style="width: 40px; height: 40px; object-fit: cover; border-radius: 4px; flex-shrink: 0;" />
                                        })}
                                        <div style="flex: 1; min-width: 0;">
                                            <div style="font-weight: 600;">{p.name.clone()}</div>
                                            <div style="font-size: 0.85rem; color: var(--color-text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{p.description.clone()}</div>
                                        </div>
                                        {if is_active {
                                            view! {
                                                <>
                                                    <span style="font-size: 0.8rem; color: #4ade80;">"active"</span>
                                                    <button on:click=move |_| {
                                                        spawn_local(async move {
                                                            match crate::api::set_active_persona(None).await {
                                                                Ok(()) => {
                                                                    set_error.set(String::new());
                                                                    me.refetch();
                                                                }
                                                                Err(e) => set_error.set(e),
                                                            }
                                                        });
                                                    }>"Deactivate"</button>
                                                </>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <button on:click=move |_| {
                                                    let id = id_for_activate.clone();
                                                    spawn_local(async move {
                                                        match crate::api::set_active_persona(Some(id)).await {
                                                            Ok(()) => {
                                                                set_error.set(String::new());
                                                                me.refetch();
                                                            }
                                                            Err(e) => set_error.set(e),
                                                        }
                                                    });
                                                }>"Activate"</button>
                                            }.into_any()
                                        }}
                                        <button on:click=move |_| {
                                            set_delete_target.set(Some((id_for_delete.clone(), name_for_delete.clone())));
                                        }>"Delete"</button>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }
                }}
            </Suspense>

            <div style="margin-top: 1rem; display: flex; flex-direction: column; gap: 0.5rem;">
                <input
                    placeholder="Persona name"
                    prop:value=new_name
                    on:input=move |ev| set_new_name.set(event_target_value(&ev))
                    style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); color: #fff; padding: 0.5rem;"
                />
                <textarea
                    placeholder="Describe this persona..."
                    prop:value=new_description
                    on:input=move |ev| set_new_description.set(event_target_value(&ev))
                    style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); color: #fff; padding: 0.5rem; min-height: 80px;"
                ></textarea>
                {move || (!error.get().is_empty()).then(|| view! { <div style="color: #f87171;">{error.get()}</div> })}
                <button on:click=create disabled=move || creating.get()>
                    {move || if creating.get() { "Creating..." } else { "+ New persona" }}
                </button>
            </div>

            {move || delete_target.get().map(|(_, name)| {
                view! {
                    <div class="modal-backdrop" on:click=move |_| set_delete_target.set(None)>
                        <div class="modal-box" style="max-width: 420px;" on:click=|ev| ev.stop_propagation()>
                            <div class="modal-header">
                                <h2>"Delete Persona"</h2>
                                <button class="icon-btn" on:click=move |_| set_delete_target.set(None)>
                                    <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2">
                                        <line x1="18" y1="6" x2="6" y2="18"></line>
                                        <line x1="6" y1="6" x2="18" y2="18"></line>
                                    </svg>
                                </button>
                            </div>
                            <div class="modal-body" style="padding: 1rem; display: flex; flex-direction: column; gap: 1rem;">
                                <p>"Delete \"" {name} "\"? This can't be undone."</p>
                                <button
                                    class="btn danger"
                                    style="background: var(--color-error); color: white;"
                                    disabled=move || deleting.get()
                                    on:click=confirm_delete
                                >
                                    {move || if deleting.get() { "Deleting..." } else { "Delete Persona" }}
                                </button>
                            </div>
                        </div>
                    </div>
                }
            })}
        </div>
    }
}
