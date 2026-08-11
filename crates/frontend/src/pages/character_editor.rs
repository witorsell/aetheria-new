use crate::api::{self, CharacterInput};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};
use std::collections::HashSet;

#[derive(Clone, PartialEq, Eq)]
enum EditorTab {
    General,
    Prompting,
    Greetings,
    Lorebooks,
}

#[component]
pub fn CharacterEditorPage() -> impl IntoView {
    let settings = leptos::prelude::LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);
    let params = use_params_map();
    let navigate = use_navigate();
    let navigate_for_save = navigate.clone();
    let navigate_for_back = navigate.clone();
    let navigate_for_delete = navigate.clone();
    let character_id = Memo::new(move |_| params.with(|p| p.get("id").unwrap_or_default()));

    let is_new = move || character_id.get() == "new" || character_id.get().is_empty();

    let (name, set_name) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (scenario, set_scenario) = signal(String::new());
    let (personality, set_personality) = signal(String::new());
    let (first_message, set_first_message) = signal(String::new());
    let (system_prompt, set_system_prompt) = signal(String::new());
    let (post_history_instructions, set_post_history_instructions) = signal(String::new());
    let (prefill, set_prefill) = signal(String::new());
    let (insert_depth_prompt, set_insert_depth_prompt) = signal(String::new());
    let (insert_depth, set_insert_depth) = signal(3i32);
    let (sample_chat, set_sample_chat) = signal(String::new());
    let (persona, set_persona) = signal(String::new());
    let (talkativeness, set_talkativeness) = signal(0.5f64);
    let (avatar_url, set_avatar_url) = signal(Option::<String>::None);
    let (alternate_greetings, set_alternate_greetings) = signal(Vec::<api::AlternateGreeting>::new());
    let (all_lorebooks, set_all_lorebooks) = signal(Vec::<api::Lorebook>::new());
    let (selected_lorebooks, set_selected_lorebooks) = signal(HashSet::<String>::new());
    let (new_greeting, set_new_greeting) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (success, set_success) = signal(Option::<String>::None);
    let (tab, set_tab) = signal(EditorTab::General);
    let (loading, set_loading) = signal(true);

    let _ = LocalResource::new(move || {
        let cid = character_id.get();
        async move {
            if !cid.is_empty() && cid != "new" {
                set_loading.set(true);
                match api::get_character(&cid).await {
                    Ok(c) => {
                        set_name.set(c.name);
                        set_description.set(c.description);
                        set_scenario.set(c.scenario);
                        set_personality.set(c.personality);
                        set_first_message.set(c.first_message);
                        set_system_prompt.set(c.system_prompt);
                        set_post_history_instructions.set(c.post_history_instructions);
                        set_prefill.set(c.prefill);
                        set_insert_depth_prompt.set(c.insert_depth_prompt);
                        set_insert_depth.set(c.insert_depth);
                        set_sample_chat.set(c.sample_chat);
                        set_persona.set(c.persona);
                        set_talkativeness.set(c.talkativeness);
                        set_avatar_url.set(c.avatar_url);
                        match api::list_alternate_greetings(&cid).await {
                            Ok(g) => set_alternate_greetings.set(g),
                            Err(_) => {}
                        }
                        match api::get_character_lorebooks(&cid).await {
                            Ok(ids) => set_selected_lorebooks.set(ids.into_iter().collect()),
                            Err(_) => {}
                        }
                    }
                    Err(e) => set_error.set(Some(e)),
                }
                set_loading.set(false);
            } else {
                set_loading.set(false);
            }
            match api::list_lorebooks().await {
                Ok(lbs) => set_all_lorebooks.set(lbs),
                Err(_) => {}
            }
        }
    });

    view! {
        <div class="editor-container">
            <Show when=move || loading.get()>
                <div class="loading">"Loading character..."</div>
            </Show>

            <Show when=move || !loading.get()>
                <h1>{move || if is_new() { "New Character" } else { "Edit Character" }}</h1>

                <div class="editor-tabs">
                    <button
                        class:active=move || tab.get() == EditorTab::General
                        on:click=move |_| set_tab.set(EditorTab::General)
                    >"General"</button>
                    <button
                        class:active=move || tab.get() == EditorTab::Prompting
                        on:click=move |_| set_tab.set(EditorTab::Prompting)
                    >"Prompting"</button>
                    <button
                        class:active=move || tab.get() == EditorTab::Greetings
                        on:click=move |_| set_tab.set(EditorTab::Greetings)
                    >"Greetings"</button>
                    <button
                        class:active=move || tab.get() == EditorTab::Lorebooks
                        on:click=move |_| set_tab.set(EditorTab::Lorebooks)
                    >"Grimoires"</button>
                </div>

                <form on:submit={
                    let nav = navigate_for_save.clone();
                    move |ev: leptos::ev::SubmitEvent| {
                    ev.prevent_default();
                    let cid = character_id.get_untracked();

                    let n = name.get_untracked();
                    let d = description.get_untracked();
                    let p = personality.get_untracked();
                    let s = scenario.get_untracked();
                    let fm = first_message.get_untracked();
                    let av = avatar_url.get_untracked();
                    let sc = sample_chat.get_untracked();
                    let sp = system_prompt.get_untracked();
                    let phi = post_history_instructions.get_untracked();
                    let pr = prefill.get_untracked();
                    let idp = insert_depth_prompt.get_untracked();
                    let ind = insert_depth.get_untracked();
                    let per = persona.get_untracked();
                    let talk = talkativeness.get_untracked();

                    spawn_local({
                        let nav = nav.clone();
                        async move {
                        let input = CharacterInput {
                            name: &n,
                            description: if d.is_empty() { None } else { Some(&d) },
                            personality: if p.is_empty() { None } else { Some(&p) },
                            scenario: if s.is_empty() { None } else { Some(&s) },
                            first_message: if fm.is_empty() { None } else { Some(&fm) },
                            avatar_url: av.as_deref(),
                            sample_chat: if sc.is_empty() { None } else { Some(&sc) },
                            system_prompt: if sp.is_empty() { None } else { Some(&sp) },
                            post_history_instructions: if phi.is_empty() { None } else { Some(&phi) },
                            prefill: if pr.is_empty() { None } else { Some(&pr) },
                            insert_depth_prompt: if idp.is_empty() { None } else { Some(&idp) },
                            insert_depth: Some(ind),
                            persona: if per.is_empty() { None } else { Some(&per) },
                            talkativeness: Some(talk),
                            extensions: None,
                            folder_id: None,
                        };

                        set_error.set(None);
                        set_success.set(None);

                        let result = if !cid.is_empty() && cid != "new" {
                            api::update_character(&cid, input).await.map(|_| cid.clone())
                        } else {
                            api::create_character(input).await.map(|c| c.id)
                        };

                        match result {
                            Ok(id) => {
                                set_success.set(Some("Character saved!".to_string()));
                                if cid.is_empty() || cid == "new" {
                                    nav(&format!("/characters/{id}/edit"), Default::default());
                                }
                                let selected_lbs: Vec<String> = selected_lorebooks.get_untracked().into_iter().collect();
                                let _ = api::set_character_lorebooks(&id, selected_lbs).await;
                            }
                            Err(e) => set_error.set(Some(e)),
                        }
                        }
                    });
                }}>

                    <Show when=move || tab.get() == EditorTab::General>
                        <div class="tab-content">
                            <label>
                                "Name"
                                <input type="text" prop:value=name
                                    on:input=move |ev| set_name.set(event_target_value(&ev))
                                    required />
                            </label>

                            <label>
                                "Avatar Image"
                                {if character_id.get() == "new" || character_id.get().is_empty() {
                                    view! { <div class="field-help">"Save the character first to upload an avatar."</div> }.into_any()
                                } else {
                                    view! {
                                        <input type="file" accept="image/*"
                                            on:change={
                                                let character_id = character_id.clone();
                                                let set_avatar_url = set_avatar_url.clone();
                                                let set_error = set_error.clone();
                                                move |ev| {
                                                    let id_str = character_id.get_untracked();
                                                    let target = event_target::<web_sys::HtmlInputElement>(&ev);
                                                    if let Some(files) = target.files() {
                                                        if let Some(file) = files.get(0) {
                                                            let form_data = web_sys::FormData::new().unwrap();
                                                            form_data.append_with_blob("avatar", &file).unwrap();
                                                            let url = format!("/api/characters/{}/avatar", id_str);
                                                            let set_avatar_url = set_avatar_url.clone();
                                                            let set_error = set_error.clone();
                                                            leptos::task::spawn_local(async move {
                                                                let req = web_sys::RequestInit::new();
                                                                req.set_method("POST");
                                                                req.set_body(&form_data);
                                                                let request = web_sys::Request::new_with_str_and_init(&url, &req).unwrap();
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
                                                                            set_error.set(Some("Failed to upload avatar".to_string()));
                                                                        }
                                                                    }
                                                                    Err(_) => set_error.set(Some("Network error".to_string())),
                                                                }
                                                            });
                                                        }
                                                    }
                                                }
                                            }
                                        />
                                    }.into_any()
                                }}
                            </label>
                            <Show when=move || avatar_url.get().is_some()>
                                <div class="avatar-preview">
                                    {move || {
                                        if !forbid_media() {
                                            view!{ <img src=move || avatar_url.get().unwrap_or_default() alt="Avatar preview" /> }.into_any()
                                        } else {
                                            view!{ <div>"Media Forbidden"</div> }.into_any()
                                        }
                                    }}
                                </div>
                            </Show>

                            <label>
                                "Description"
                                <textarea prop:value=description rows="3"
                                    on:input=move |ev| set_description.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Personality"
                                <textarea prop:value=personality rows="3"
                                    on:input=move |ev| set_personality.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Scenario"
                                <textarea prop:value=scenario rows="3"
                                    on:input=move |ev| set_scenario.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "First Message (Greeting)"
                                <textarea prop:value=first_message rows="4"
                                    on:input=move |ev| set_first_message.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Sample Chat"
                                <textarea prop:value=sample_chat rows="6"
                                    placeholder="Example conversation to guide the model's style..."
                                    on:input=move |ev| set_sample_chat.set(event_target_value(&ev)) />
                            </label>

                            <div class="field">
                                <label>"Talkativeness"</label>
                                <input
                                    type="number"
                                    min="0"
                                    max="1"
                                    step="0.05"
                                    prop:value=move || talkativeness.get()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                            set_talkativeness.set(v.clamp(0.0, 1.0));
                                        }
                                    }
                                />
                                <p style="font-size: 0.8rem; color: var(--color-text-muted);">"How often this character jumps into a group conversation on its own (0 = never, 1 = always). Only matters in Natural-mode group chats."</p>
                            </div>
                        </div>
                    </Show>

                    <Show when=move || tab.get() == EditorTab::Prompting>
                        <div class="tab-content">
                            <label>
                                "System Prompt"
                                <textarea prop:value=system_prompt rows="4"
                                    placeholder="Custom system prompt (overrides global)…"
                                    on:input=move |ev| set_system_prompt.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Post-History Instructions"
                                <textarea prop:value=post_history_instructions rows="3"
                                    placeholder="Instructions injected after the conversation history…"
                                    on:input=move |ev| set_post_history_instructions.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Prefill (Assistant Start)"
                                <textarea prop:value=prefill rows="2"
                                    placeholder="Text to prefill in the assistant's response…"
                                    on:input=move |ev| set_prefill.set(event_target_value(&ev)) />
                            </label>

                            <label>
                                "Insert Depth Prompt"
                                <textarea prop:value=insert_depth_prompt rows="2"
                                    placeholder="Enter instructions to inject deeper into context..."
                                    on:input=move |ev| set_insert_depth_prompt.set(event_target_value(&ev)) />
                            </label>
                            <label>
                                "Insert Depth"
                                <input type="number" prop:value=insert_depth
                                    style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.5rem; width: 100%; border-radius: 4px;"
                                    on:input=move |ev| {
                                        if let Ok(val) = event_target_value(&ev).parse::<i32>() {
                                            set_insert_depth.set(val);
                                        }
                                    } />
                            </label>
                        </div>
                    </Show>

                    <Show when=move || tab.get() == EditorTab::Lorebooks>
                        <div class="tab-content">
                            <label style="margin-bottom: 0.5rem; display: block; font-weight: 500;">"Attached Grimoires"</label>
                            <div style="display: flex; flex-direction: column; gap: 0.5rem; max-height: 400px; overflow-y: auto; padding: 0.5rem; border: 1px solid var(--color-border);">
                                {move || all_lorebooks.get().into_iter().map(|lb| {
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
                        </div>
                    </Show>

                    <Show when=move || tab.get() == EditorTab::Greetings>
                        <div class="tab-content">
                            <p class="help-text">"Alternate greetings let a character start conversations in different ways. They're picked randomly or shown as swipes."</p>

                            <div class="greetings-list">
                                <Show when=move || alternate_greetings.get().is_empty()>
                                    <p class="muted">"No alternate greetings yet."</p>
                                </Show>
                                <For
                                    each=move || alternate_greetings.get()
                                    key=|g: &api::AlternateGreeting| g.id.clone()
                                    children=move |g: api::AlternateGreeting| {
                                        let gid = g.id.clone();
                                        let greeting_text = g.greeting.clone();
                                        view! {
                                            <div class="greeting-row">
                                                <textarea class="greeting-textarea"
                                                    prop:value=greeting_text
                                                    on:change={
                                                        let gid_for_update = gid.clone();
                                                        move |ev| {
                                                            let new_val = event_target_value(&ev);
                                                            let cid = character_id.get_untracked();
                                                            let gid2 = gid_for_update.clone();
                                                            if !cid.is_empty() && cid != "new" {
                                                                spawn_local(async move {
                                                                    let _ = api::update_alternate_greeting(&cid, &gid2, &new_val).await;
                                                                });
                                                            }
                                                        }
                                                    }
                                                ></textarea>
                                                <button type="button" class="danger small"
                                                    on:click={
                                                        let gid_for_del = gid.clone();
                                                        move |_| {
                                                        let cid = character_id.get_untracked();
                                                        if cid.is_empty() || cid == "new" {
                                                            return;
                                                        }
                                                        let gid2 = gid_for_del.clone();
                                                        spawn_local(async move {
                                                            match api::delete_alternate_greeting(&cid, &gid2).await {
                                                                Ok(()) => {
                                                                    set_alternate_greetings.update(|list| list.retain(|g| g.id != gid2));
                                                                }
                                                                Err(e) => set_error.set(Some(e)),
                                                            }
                                                        });
                                                    }}
                                                    >
                                                    "Delete"
                                                </button>
                                            </div>
                                        }
                                    }
                                />
                            </div>

                            <div class="add-greeting">
                                <textarea
                                    class="add-greeting-textarea"
                                    placeholder="Add alternate greeting…"
                                    prop:value=new_greeting
                                    on:input=move |ev| set_new_greeting.set(event_target_value(&ev))></textarea>
                                <button type="button" class="secondary" on:click=move |_| {
                                    let greeting = new_greeting.get_untracked();
                                    let cid = character_id.get_untracked();
                                    if greeting.is_empty() || cid.is_empty() || cid == "new" {
                                        return;
                                    }
                                    spawn_local(async move {
                                        match api::add_alternate_greeting(&cid, &greeting).await {
                                            Ok(g) => {
                                                set_alternate_greetings.update(|list| list.push(g));
                                                set_new_greeting.set(String::new());
                                            }
                                            Err(e) => set_error.set(Some(e)),
                                        }
                                    });
                                }>
                                    "Add"
                                </button>
                            </div>
                        </div>
                    </Show>

                    {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
                    {move || success.get().map(|s| view! { <p class="success">{s}</p> })}

                    <div class="form-actions">
                        <button type="submit" class="primary">"Save Character"</button>

                        <button type="button" 
                            style=move || if is_new() { "display: none;" } else { "background-color: #8b0000; color: white; margin-left: 10px;" }
                            on:click={
                                let nav = navigate_for_delete.clone();
                                move |_| {
                                    let id = character_id.get_untracked();
                                    if let Some(win) = web_sys::window() {
                                        if win.confirm_with_message("Delete this character forever?").unwrap_or(false) {
                                            let nav = nav.clone();
                                            spawn_local(async move {
                                                if let Ok(_) = crate::api::delete_character(&id).await {
                                                    nav("/characters", Default::default());
                                                }
                                            });
                                        }
                                    }
                                }
                            }>
                            "Delete"
                        </button>

                        <button type="button" class="ghost" on:click={
                            let navigate = navigate_for_back.clone();
                            move |_| navigate("/characters", Default::default())
                        }>
                            "Back to List"
                        </button>
                    </div>
                </form>
            </Show>
        </div>
    }
}
