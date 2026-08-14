use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;

use super::settings_forms::{UserProfileForm, PersonaManager, SPEED_PRESETS, speed_to_preset_index};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let settings = LocalResource::new(|| async move { crate::api::get_settings().await });

    let (api_base_url, set_api_base_url) = signal(String::new());
    let (api_key, set_api_key) = signal(String::new());
    let (model_name, set_model_name) = signal(String::new());
    let (system_prompt, set_system_prompt) = signal(String::new());
    let (context_limit, set_context_limit) = signal(8192i64);
    let (post_history_instructions, set_post_history_instructions) = signal(String::new());
    let (forbid_external_media, set_forbid_external_media) = signal(false);
    let (provider_type, set_provider_type) = signal(String::from("openai"));
    let (summary_provider_type, set_summary_provider_type) = signal(String::new());
    let (summary_api_base_url, set_summary_api_base_url) = signal(String::new());
    let (summary_api_key, set_summary_api_key) = signal(String::new());
    let (has_summary_key, set_has_summary_key) = signal(false);
    let (summary_model_name, set_summary_model_name) = signal(String::new());
    let (summary_context_limit, set_summary_context_limit) = signal(String::new());
    let (embedding_source, set_embedding_source) = signal(String::new());
    let (embedding_api_base_url, set_embedding_api_base_url) = signal(String::new());
    let (embedding_api_key, set_embedding_api_key) = signal(String::new());
    let (has_embedding_key, set_has_embedding_key) = signal(false);
    let (embedding_model_name, set_embedding_model_name) = signal(String::new());
    let (rag_top_k, set_rag_top_k) = signal(5i64);
    let (rag_score_threshold, set_rag_score_threshold) = signal(0.5f64);
    let (temperature, set_temperature) = signal(1.0f64);
    let (top_p, set_top_p) = signal(1.0f64);
    let (top_k, set_top_k) = signal(0i64);
    let (frequency_penalty, set_frequency_penalty) = signal(0.0f64);
    let (presence_penalty, set_presence_penalty) = signal(0.0f64);
    let (max_response_tokens, set_max_response_tokens) = signal(0i64);
    let (reasoning_effort, set_reasoning_effort) = signal(String::new());
    let (speed_index, set_speed_index) = signal(speed_to_preset_index(crate::api::get_text_speed()));
    let (has_key, set_has_key) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (saved, set_saved) = signal(false);
    let (models, set_models) = signal(Vec::<(String, String)>::new());
    let (loading_models, set_loading_models) = signal(false);
    let (subscription_only, set_subscription_only) = signal(crate::api::get_subscription_only_models());
    let (export_import_error, set_export_import_error) = signal(Option::<String>::None);
    let (import_done, set_import_done) = signal(false);
    let (account_export_import_error, set_account_export_import_error) = signal(Option::<String>::None);
    let (account_import_done, set_account_import_done) = signal(false);
    let (delete_confirm_open, set_delete_confirm_open) = signal(false);
    let (delete_confirm_text, set_delete_confirm_text) = signal(String::new());
    let (delete_in_progress, set_delete_in_progress) = signal(false);
    let (delete_error, set_delete_error) = signal(Option::<String>::None);

    let me_resource = LocalResource::new(|| async move { crate::api::fetch_me().await });

    let (username, set_username) = signal(String::new());
    let (display_name, set_display_name) = signal(String::new());
    let (avatar_url, set_avatar_url) = signal(Option::<String>::None);
    let (user_error, set_user_error) = signal(Option::<String>::None);
    let (user_saved, set_user_saved) = signal(false);

    Effect::new(move |_| {
        if let Some(Ok(view)) = settings.get() {
            set_api_base_url.set(view.api_base_url);
            set_model_name.set(view.model_name);
            set_system_prompt.set(view.system_prompt);
            set_has_key.set(view.has_api_key);
            set_context_limit.set(view.context_limit);
            set_post_history_instructions.set(view.post_history_instructions);
            set_forbid_external_media.set(view.forbid_external_media);
            set_provider_type.set(view.provider_type);
            set_summary_provider_type.set(view.summary_provider_type);
            set_summary_api_base_url.set(view.summary_api_base_url);
            set_has_summary_key.set(view.has_summary_api_key);
            set_summary_model_name.set(view.summary_model_name);
            set_summary_context_limit.set(view.summary_context_limit.map(|n| n.to_string()).unwrap_or_default());
            set_embedding_source.set(view.embedding_source);
            set_embedding_api_base_url.set(view.embedding_api_base_url);
            set_has_embedding_key.set(view.has_embedding_api_key);
            set_embedding_model_name.set(view.embedding_model_name);
            set_rag_top_k.set(view.rag_top_k);
            set_rag_score_threshold.set(view.rag_score_threshold);
            set_temperature.set(view.temperature);
            set_top_p.set(view.top_p);
            set_top_k.set(view.top_k);
            set_frequency_penalty.set(view.frequency_penalty);
            set_presence_penalty.set(view.presence_penalty);
            set_max_response_tokens.set(view.max_response_tokens);
            set_reasoning_effort.set(view.reasoning_effort);
        }
    });

    Effect::new(move |_| {
        if let Some(Ok(me)) = me_resource.get() {
            set_username.set(me.username);
            set_display_name.set(me.display_name.unwrap_or_default());
            set_avatar_url.set(me.avatar_url);
        }
    });

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_saved.set(false);
        let key = api_key.get_untracked();
        let summary_key = summary_api_key.get_untracked();
        let summary_limit_str = summary_context_limit.get_untracked();
        let embedding_key = embedding_api_key.get_untracked();
        spawn_local(async move {
            let key_arg = if key.is_empty() { None } else { Some(key.clone()) };
            let summary_key_arg = if summary_key.is_empty() { None } else { Some(summary_key.clone()) };
            let summary_limit_arg = summary_limit_str.trim().parse::<i64>().ok();
            let embedding_key_arg = if embedding_key.is_empty() { None } else { Some(embedding_key.clone()) };
            let req = crate::api::UpdateSettingsRequest {
                api_base_url: api_base_url.get_untracked(),
                api_key: key_arg.clone(),
                model_name: model_name.get_untracked(),
                system_prompt: system_prompt.get_untracked(),
                context_limit: context_limit.get_untracked(),
                post_history_instructions: post_history_instructions.get_untracked(),
                forbid_external_media: forbid_external_media.get_untracked(),
                provider_type: provider_type.get_untracked(),
                summary_provider_type: summary_provider_type.get_untracked(),
                summary_api_base_url: summary_api_base_url.get_untracked(),
                summary_api_key: summary_key_arg.clone(),
                summary_model_name: summary_model_name.get_untracked(),
                summary_context_limit: summary_limit_arg,
                embedding_source: embedding_source.get_untracked(),
                embedding_api_base_url: embedding_api_base_url.get_untracked(),
                embedding_api_key: embedding_key_arg.clone(),
                embedding_model_name: embedding_model_name.get_untracked(),
                rag_top_k: rag_top_k.get_untracked(),
                rag_score_threshold: rag_score_threshold.get_untracked(),
                temperature: temperature.get_untracked(),
                top_p: top_p.get_untracked(),
                top_k: top_k.get_untracked(),
                frequency_penalty: frequency_penalty.get_untracked(),
                presence_penalty: presence_penalty.get_untracked(),
                max_response_tokens: max_response_tokens.get_untracked(),
                reasoning_effort: reasoning_effort.get_untracked(),
            };
            match crate::api::update_settings(req).await {
                Ok(()) => {
                    set_error.set(None);
                    set_saved.set(true);
                    set_api_key.set(String::new());
                    set_has_key.set(true);
                    set_summary_api_key.set(String::new());
                    if summary_key_arg.is_some() {
                        set_has_summary_key.set(true);
                    }
                    set_embedding_api_key.set(String::new());
                    if embedding_key_arg.is_some() {
                        set_has_embedding_key.set(true);
                    }
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let on_export = move |_: leptos::ev::MouseEvent| {
        set_export_import_error.set(None);
        spawn_local(async move {
            match crate::api::export_settings().await {
                Ok(export) => {
                    let json = serde_json::to_string_pretty(&export).unwrap_or_default();
                    if let Err(e) = crate::api::download_text_file("aetheria-settings.json", "application/json", &json) {
                        set_export_import_error.set(Some(e));
                    }
                }
                Err(e) => set_export_import_error.set(Some(e)),
            }
        });
    };

    let on_import_file_selected = move |ev: leptos::ev::Event| {
        set_export_import_error.set(None);
        set_import_done.set(false);
        let target = event_target::<web_sys::HtmlInputElement>(&ev);
        let Some(files) = target.files() else { return };
        let Some(file) = files.get(0) else { return };
        spawn_local(async move {
            let text = match wasm_bindgen_futures::JsFuture::from(file.text()).await {
                Ok(v) => v.as_string().unwrap_or_default(),
                Err(_) => {
                    set_export_import_error.set(Some("Could not read the selected file".to_string()));
                    return;
                }
            };
            let export: crate::api::SettingsExport = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    set_export_import_error.set(Some(format!("Not a valid settings export: {e}")));
                    return;
                }
            };
            match crate::api::import_settings(&export).await {
                Ok(()) => {
                    set_import_done.set(true);
                    // a full reload is the simplest way to get every field
                    // (provider, sampling, RAG, summarization, embedding)
                    // back in sync with what was just imported.
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(e) => set_export_import_error.set(Some(e)),
            }
        });
    };

    let on_export_account = move |_: leptos::ev::MouseEvent| {
        set_account_export_import_error.set(None);
        spawn_local(async move {
            match crate::api::export_account().await {
                Ok(json) => {
                    if let Err(e) = crate::api::download_text_file("aetheria-account-export.json", "application/json", &json) {
                        set_account_export_import_error.set(Some(e));
                    }
                }
                Err(e) => set_account_export_import_error.set(Some(e)),
            }
        });
    };

    let on_import_account_file_selected = move |ev: leptos::ev::Event| {
        set_account_export_import_error.set(None);
        set_account_import_done.set(false);
        let target = event_target::<web_sys::HtmlInputElement>(&ev);
        let Some(files) = target.files() else { return };
        let Some(file) = files.get(0) else { return };
        spawn_local(async move {
            let text = match wasm_bindgen_futures::JsFuture::from(file.text()).await {
                Ok(v) => v.as_string().unwrap_or_default(),
                Err(_) => {
                    set_account_export_import_error.set(Some("Could not read the selected file".to_string()));
                    return;
                }
            };
            match crate::api::import_account(&text).await {
                Ok(()) => {
                    set_account_import_done.set(true);
                    // a full reload is the simplest way to get every imported
                    // character/chat/lorebook/etc. showing up everywhere.
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().reload();
                    }
                }
                Err(e) => set_account_export_import_error.set(Some(e)),
            }
        });
    };

    let on_confirm_delete = move |_: leptos::ev::MouseEvent| {
        set_delete_error.set(None);
        set_delete_in_progress.set(true);
        let typed = delete_confirm_text.get_untracked();
        spawn_local(async move {
            match crate::api::delete_account_data(&typed).await {
                Ok(()) => {
                    if let Some(window) = web_sys::window() {
                        let _ = window.location().set_href("/");
                    }
                }
                Err(e) => {
                    set_delete_error.set(Some(e));
                    set_delete_in_progress.set(false);
                }
            }
        });
    };

    let load_models = move |_| {
        set_error.set(None);
        set_loading_models.set(true);
        let subscription_only = subscription_only.get_untracked();
        spawn_local(async move {
            match crate::api::list_models(subscription_only).await {
                Ok(list) => {
                    let ids: Vec<String> = list.into_iter().map(|m| m.id).collect();

                    // strip the namespace prefix for display, but only when
                    // that's still unique, otherwise two real, different
                    // models (e.g. a PAYG-only "TEE/" variant alongside the
                    // regular one) would look like duplicate entries.
                    let mut suffix_counts: HashMap<String, u32> = HashMap::new();
                    for id in &ids {
                        let suffix = id.rsplit('/').next().unwrap_or(id).to_string();
                        *suffix_counts.entry(suffix).or_insert(0) += 1;
                    }
                    let mut items: Vec<(String, String)> = ids
                        .into_iter()
                        .map(|id| {
                            let suffix = id.rsplit('/').next().unwrap_or(&id).to_string();
                            let display = if suffix_counts.get(&suffix) == Some(&1) {
                                suffix
                            } else {
                                id.clone()
                            };
                            (id, display)
                        })
                        .collect();
                    items.sort_by(|(_, a), (_, b)| a.to_lowercase().cmp(&b.to_lowercase()));

                    set_models.set(items);
                    set_error.set(None);
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_loading_models.set(false);
        });
    };

    let on_speed_change = move |ev: leptos::ev::Event| {
        if let Ok(index) = event_target_value(&ev).parse::<usize>() {
            let index = index.min(SPEED_PRESETS.len() - 1);
            set_speed_index.set(index);
            crate::api::set_text_speed(SPEED_PRESETS[index].0);
        }
    };

    view! {
        <div class="settings-page" style="padding: 2rem; max-width: 800px; margin: 0 auto;">
            <p style="margin-bottom: 2rem;"><a class="back-link" href="/characters" style="color: var(--color-text-muted); text-decoration: none; font-family: monospace; text-transform: uppercase; letter-spacing: 0.1em; font-size: 0.8rem; border-bottom: 1px dotted var(--color-border); padding-bottom: 2px;">"< BACK TO CHARACTERS"</a></p>
            
            <div style="margin-bottom: 3rem;">
                <h1 style="font-family: var(--font-heading); font-size: 3rem; font-weight: 300; margin: 0; color: #fff; letter-spacing: -0.02em;">"User Profile"</h1>
                <p style="color: var(--color-text-muted); font-family: monospace; margin-top: 0.5rem; text-transform: uppercase; font-size: 0.85rem; letter-spacing: 0.05em;">"Configure your display name and global persona."</p>
            </div>

            <UserProfileForm
                display_name=display_name.into()
                set_display_name
                avatar_url=avatar_url.into()
                set_avatar_url
                user_error=user_error.into()
                set_user_error
                user_saved=user_saved.into()
                set_user_saved
                forbid_external_media=forbid_external_media.into()
            />

            <PersonaManager />

            <div style="margin-bottom: 3rem;">
                <h1 style="font-family: var(--font-heading); font-size: 3rem; font-weight: 300; margin: 0; color: #fff; letter-spacing: -0.02em;">"System Manifest"</h1>
                <p style="color: var(--color-text-muted); font-family: monospace; margin-top: 0.5rem; text-transform: uppercase; font-size: 0.85rem; letter-spacing: 0.05em;">"Configure core parameters and provider connections."</p>
            </div>

            <div style="display: flex; align-items: center; gap: 1.5rem; margin-bottom: 3rem; flex-wrap: wrap;">
                <button
                    type="button"
                    on:click=on_export
                    style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.6rem 1.25rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; letter-spacing: 0.05em;"
                >
                    "Export Settings"
                </button>

                <label style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.6rem 1.25rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; letter-spacing: 0.05em;">
                    "Import Settings"
                    <input type="file" accept="application/json" style="display: none;" on:change=on_import_file_selected />
                </label>

                <span style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; opacity: 0.7;">"Provider, sampling, memory & RAG config only. Never includes API keys, display name, persona, or avatar."</span>

                {move || import_done.get().then(|| view! { <span style="color: #4CAF50; font-family: monospace; font-size: 0.85rem;">"✓ IMPORTED"</span> })}
                {move || export_import_error.get().map(|e| view! { <span style="color: #ff4444; font-family: monospace; font-size: 0.85rem;">{e}</span> })}
            </div>

            {move || delete_confirm_open.get().then(|| view! {
                <div class="modal-backdrop" on:click=move |_| { set_delete_confirm_open.set(false); set_delete_confirm_text.set(String::new()); }>
                    <div class="modal-box" style="max-width: 480px;" on:click=|ev| ev.stop_propagation()>
                        <div class="modal-header">
                            <h2>"Delete Everything"</h2>
                            <button class="icon-btn" on:click=move |_| { set_delete_confirm_open.set(false); set_delete_confirm_text.set(String::new()); }>
                                <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2">
                                    <line x1="18" y1="6" x2="6" y2="18"></line>
                                    <line x1="6" y1="6" x2="18" y2="18"></line>
                                </svg>
                            </button>
                        </div>
                        <div class="modal-body" style="padding: 1rem; display: flex; flex-direction: column; gap: 1rem;">
                            <p>"This permanently deletes every character, chat, group chat, lorebook, preset, regex script, and theme on your account. It does NOT touch your account itself, your login, or your API keys/settings."</p>
                            <p style="font-weight: bold;">"This can't be undone. Export your data first if you want a backup."</p>
                            <p>"Type your username (" {move || username.get()} ") to confirm:"</p>
                            <input
                                type="text"
                                prop:value=delete_confirm_text
                                on:input=move |ev| set_delete_confirm_text.set(event_target_value(&ev))
                                style="padding: 0.5rem; font-family: monospace;"
                            />
                            {move || delete_error.get().map(|e| view! { <span style="color: #ff4444;">{e}</span> })}
                            <button
                                class="btn danger"
                                style="background: var(--color-error); color: white;"
                                disabled=move || {
                                    let expected = username.get();
                                    expected.is_empty() || delete_confirm_text.get() != expected || delete_in_progress.get()
                                }
                                on:click=on_confirm_delete
                            >
                                {move || if delete_in_progress.get() { "Deleting..." } else { "Permanently Delete Everything" }}
                            </button>
                        </div>
                    </div>
                </div>
            })}

            <form class="settings-form" on:submit=on_submit style="display: flex; flex-direction: column; gap: 2.5rem;">
                
                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Provider Type"</label>
                    <div style="position: relative; width: 100%;">
                        <select
                            prop:value=provider_type
                            on:change=move |ev| set_provider_type.set(event_target_value(&ev))
                            style="appearance: none; background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.1); border-radius: 8px; color: #fff; font-family: var(--font-body); padding: 0.75rem 1rem; font-size: 1rem; outline: none; width: 100%; transition: all 0.3s ease; cursor: pointer; box-shadow: inset 0 2px 4px rgba(0,0,0,0.2);"
                            on:focus=|_| {}
                        >
                            <option value="openai" style="background: #1a1a1a; color: #fff;">"OpenAI Compatible"</option>
                            <option value="anthropic" style="background: #1a1a1a; color: #fff;">"Anthropic"</option>
                            <option value="gemini" style="background: #1a1a1a; color: #fff;">"Google Gemini"</option>
                            <option value="novelai" style="background: #1a1a1a; color: #fff;">"NovelAI"</option>
                            <option value="kobold" style="background: #1a1a1a; color: #fff;">"KoboldCPP"</option>
                            <option value="mancer" style="background: #1a1a1a; color: #fff;">"Mancer"</option>
                            <option value="horde" style="background: #1a1a1a; color: #fff;">"AI Horde"</option>
                        </select>
                        <div style="position: absolute; right: 1rem; top: 50%; transform: translateY(-50%); pointer-events: none; color: var(--color-text-muted);">
                            <svg width="12" height="8" viewBox="0 0 12 8" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M1 1.5L6 6.5L11 1.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </div>
                </div>

                {move || {
                    if ["openai", "kobold", "mancer"].contains(&provider_type.get().as_str()) {
                        Some(view! {
                            <div class="field" style="margin: 0;">
                                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Provider (Base URL)"</label>
                                <input
                                    type="text"
                                    placeholder=""
                                    prop:value=api_base_url.clone()
                                    on:input=move |ev| set_api_base_url.set(event_target_value(&ev))
                                    style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                                />
                                <p style="color: var(--color-text-muted); font-size: 0.75rem; margin: 0.25rem 0 0 0;">"Don't include /chat/completions, just the base URL (e.g. https://openrouter.ai/api/v1)"</p>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
                
                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">
                        "API Key "
                        {move || {
                            if has_key.get() {
                                view!{ <span style="opacity: 0.5; font-size: 0.7rem;">"(STORED: LEAVE BLANK TO KEEP)"</span> }.into_view()
                            } else {
                                view!{ <span style="color: #ff4444; font-size: 0.7rem;">"(NOT SET)"</span> }.into_view()
                            }
                        }}
                    </label>
                    <input
                        type="password"
                        placeholder="sk-..."
                        prop:value=api_key
                        on:input=move |ev| set_api_key.set(event_target_value(&ev))
                        style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                    />
                </div>
                
                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Model Name"</label>
                    <div class="model-name-row" style="display: flex; gap: 1rem; align-items: flex-end;">
                        <input
                            type="text"
                            placeholder="gpt-4o-mini"
                            prop:value=model_name
                            on:input=move |ev| set_model_name.set(event_target_value(&ev))
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; flex: 1; transition: border-color 0.2s;"
                        />
                        <button
                            type="button"
                            on:click=load_models
                            disabled=move || loading_models.get()
                            style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.5rem 1rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; height: 36px; display: flex; align-items: center; justify-content: center; min-width: 150px; transition: all 0.2s;"
                        >
                            {move || if loading_models.get() { "FETCHING..." } else { "FETCH MODELS" }}
                        </button>
                    </div>
                    
                    <label class="checkbox-field" style="display: flex; align-items: center; gap: 0.5rem; margin-top: 1rem; color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; cursor: pointer;">
                        <input
                            type="checkbox"
                            prop:checked=move || subscription_only.get()
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                set_subscription_only.set(checked);
                                crate::api::set_subscription_only_models(checked);
                            }
                            style="accent-color: #fff;"
                        />
                        "SUBSCRIPTION MODELS ONLY (NANOGPT)"
                    </label>
                    
                    {move || {
                        let list = models.get();
                        (!list.is_empty())
                            .then(|| {
                                view! {
                                    <div class="field" style="margin-top: 1.5rem;">
                                        <select
                                            on:change=move |ev| set_model_name.set(event_target_value(&ev))
                                            style="background: rgba(255,255,255,0.05); border: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.75rem; font-size: 0.9rem; outline: none; width: 100%; appearance: none; cursor: pointer;"
                                        >
                                            <option value="" style="background: var(--color-bg); color: #fff;">"SELECT A MODEL..."</option>
                                            {list
                                                .into_iter()
                                                .map(|(id, display)| {
                                                    view! { <option value=id.clone() style="background: var(--color-bg); color: #fff;">{display}</option> }
                                                })
                                                .collect_view()}
                                        </select>
                                    </div>
                                }
                            })
                    }}
                </div>
                
                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"System Prompt"</label>
                    <textarea
                        placeholder="Global system prompt..."
                        prop:value=system_prompt
                        on:input=move |ev| set_system_prompt.set(event_target_value(&ev))
                        style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); color: #fff; font-family: var(--font-body); padding: 1rem; font-size: 0.95rem; outline: none; width: 100%; min-height: 120px; resize: vertical; line-height: 1.5;"
                    ></textarea>
                </div>
                
                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Post-History Instructions"</label>
                    <textarea
                        rows="6"
                        placeholder="Enforced instructions..."
                        prop:value=post_history_instructions
                        on:input=move |ev| set_post_history_instructions.set(event_target_value(&ev))
                        style="background: rgba(0,0,0,0.2); border: 1px solid var(--color-border); color: #fff; font-family: var(--font-body); padding: 1rem; font-size: 0.95rem; outline: none; width: 100%; min-height: 200px; resize: vertical; line-height: 1.5;"
                    ></textarea>
                    <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"Sent as its own message right after the conversation history. Never dropped by the context limit."</div>
                </div>

                <div style="border-top: 1px solid var(--color-border); padding-top: 2rem;">
                    <h2 style="font-family: var(--font-heading); font-size: 1.4rem; font-weight: 400; margin: 0; color: #fff;">"Summarization Model"</h2>
                    <p style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; margin-top: 0.4rem; opacity: 0.8;">"Used to fold old chat history into a running memory summary. Leave blank to use the main provider above for this too."</p>
                </div>

                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Summarization Provider Type"</label>
                    <div style="position: relative; width: 100%;">
                        <select
                            prop:value=summary_provider_type
                            on:change=move |ev| set_summary_provider_type.set(event_target_value(&ev))
                            style="appearance: none; background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.1); border-radius: 8px; color: #fff; font-family: var(--font-body); padding: 0.75rem 1rem; font-size: 1rem; outline: none; width: 100%; transition: all 0.3s ease; cursor: pointer; box-shadow: inset 0 2px 4px rgba(0,0,0,0.2);"
                        >
                            <option value="" style="background: #1a1a1a; color: #fff;">"Same as main provider"</option>
                            <option value="openai" style="background: #1a1a1a; color: #fff;">"OpenAI Compatible"</option>
                            <option value="anthropic" style="background: #1a1a1a; color: #fff;">"Anthropic"</option>
                            <option value="gemini" style="background: #1a1a1a; color: #fff;">"Google Gemini"</option>
                            <option value="novelai" style="background: #1a1a1a; color: #fff;">"NovelAI"</option>
                            <option value="kobold" style="background: #1a1a1a; color: #fff;">"KoboldCPP"</option>
                            <option value="mancer" style="background: #1a1a1a; color: #fff;">"Mancer"</option>
                            <option value="horde" style="background: #1a1a1a; color: #fff;">"AI Horde"</option>
                        </select>
                        <div style="position: absolute; right: 1rem; top: 50%; transform: translateY(-50%); pointer-events: none; color: var(--color-text-muted);">
                            <svg width="12" height="8" viewBox="0 0 12 8" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M1 1.5L6 6.5L11 1.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </div>
                </div>

                {move || {
                    let effective = {
                        let sp = summary_provider_type.get();
                        if sp.is_empty() { provider_type.get() } else { sp }
                    };
                    if ["openai", "kobold", "mancer"].contains(&effective.as_str()) {
                        Some(view! {
                            <div class="field" style="margin: 0;">
                                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Summarization Base URL"</label>
                                <input
                                    type="text"
                                    placeholder="Same as main"
                                    prop:value=summary_api_base_url
                                    on:input=move |ev| set_summary_api_base_url.set(event_target_value(&ev))
                                    style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                                />
                            </div>
                        })
                    } else {
                        None
                    }
                }}

                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">
                        "Summarization API Key "
                        {move || {
                            if has_summary_key.get() {
                                view!{ <span style="opacity: 0.5; font-size: 0.7rem;">"(STORED: LEAVE BLANK TO KEEP)"</span> }.into_view()
                            } else {
                                view!{ <span style="opacity: 0.5; font-size: 0.7rem;">"(BLANK = USE MAIN KEY)"</span> }.into_view()
                            }
                        }}
                    </label>
                    <input
                        type="password"
                        placeholder="Same as main"
                        prop:value=summary_api_key
                        on:input=move |ev| set_summary_api_key.set(event_target_value(&ev))
                        style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                    />
                </div>

                <div style="display: flex; gap: 2rem; flex-wrap: wrap;">
                    <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Summarization Model Name"</label>
                        <input
                            type="text"
                            placeholder="Same as main"
                            prop:value=summary_model_name
                            on:input=move |ev| set_summary_model_name.set(event_target_value(&ev))
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Summarization Context Limit"</label>
                        <input
                            type="number"
                            min="0"
                            step="256"
                            placeholder="Same as main"
                            prop:value=summary_context_limit
                            on:input=move |ev| set_summary_context_limit.set(event_target_value(&ev))
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"Caps how much history goes into one summarization pass. 0 = unlimited."</div>
                    </div>
                </div>

                <div style="border-top: 1px solid var(--color-border); padding-top: 2rem;">
                    <h2 style="font-family: var(--font-heading); font-size: 1.4rem; font-weight: 400; margin: 0; color: #fff;">"Long-Term Memory"</h2>
                    <p style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; margin-top: 0.4rem; opacity: 0.8;">"Pulls old, semantically relevant messages back into context by similarity, even after they've fallen out of the token window. Off by default."</p>
                </div>

                <div class="field" style="margin: 0;">
                    <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Embedding Source"</label>
                    <div style="position: relative; width: 100%;">
                        <select
                            prop:value=embedding_source
                            on:change=move |ev| set_embedding_source.set(event_target_value(&ev))
                            style="appearance: none; background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.1); border-radius: 8px; color: #fff; font-family: var(--font-body); padding: 0.75rem 1rem; font-size: 1rem; outline: none; width: 100%; transition: all 0.3s ease; cursor: pointer; box-shadow: inset 0 2px 4px rgba(0,0,0,0.2);"
                        >
                            <option value="" style="background: #1a1a1a; color: #fff;">"Off"</option>
                            <option value="local" style="background: #1a1a1a; color: #fff;">"Local (free, runs on the server)"</option>
                            <option value="api" style="background: #1a1a1a; color: #fff;">"API (OpenAI-compatible /embeddings)"</option>
                        </select>
                        <div style="position: absolute; right: 1rem; top: 50%; transform: translateY(-50%); pointer-events: none; color: var(--color-text-muted);">
                            <svg width="12" height="8" viewBox="0 0 12 8" fill="none" xmlns="http://www.w3.org/2000/svg">
                                <path d="M1 1.5L6 6.5L11 1.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                        </div>
                    </div>
                    {move || {
                        (embedding_source.get() == "local").then(|| view! {
                            <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"Downloads a small model (~90MB) the first time it's used, then runs entirely on the server. No API key, no per-call cost."</div>
                        })
                    }}
                </div>

                {move || {
                    (embedding_source.get() == "api").then(|| view! {
                        <>
                        <div class="field" style="margin: 0;">
                            <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Embedding Base URL"</label>
                            <input
                                type="text"
                                placeholder="Same as main"
                                prop:value=embedding_api_base_url
                                on:input=move |ev| set_embedding_api_base_url.set(event_target_value(&ev))
                                style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                            />
                        </div>

                        <div class="field" style="margin: 0;">
                            <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">
                                "Embedding API Key "
                                {move || {
                                    if has_embedding_key.get() {
                                        view!{ <span style="opacity: 0.5; font-size: 0.7rem;">"(STORED: LEAVE BLANK TO KEEP)"</span> }.into_view()
                                    } else {
                                        view!{ <span style="opacity: 0.5; font-size: 0.7rem;">"(BLANK = USE MAIN KEY)"</span> }.into_view()
                                    }
                                }}
                            </label>
                            <input
                                type="password"
                                placeholder="Same as main"
                                prop:value=embedding_api_key
                                on:input=move |ev| set_embedding_api_key.set(event_target_value(&ev))
                                style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                            />
                        </div>

                        <div class="field" style="margin: 0;">
                            <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Embedding Model Name"</label>
                            <input
                                type="text"
                                placeholder="text-embedding-3-small"
                                prop:value=embedding_model_name
                                on:input=move |ev| set_embedding_model_name.set(event_target_value(&ev))
                                style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                            />
                        </div>
                        </>
                    })
                }}

                {move || {
                    (!embedding_source.get().is_empty()).then(|| view! {
                        <div style="display: flex; gap: 2rem; flex-wrap: wrap;">
                            <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Retrieval Top K"</label>
                                <input
                                    type="number"
                                    min="1"
                                    step="1"
                                    prop:value=move || rag_top_k.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(value) = event_target_value(&ev).parse::<i64>() {
                                            set_rag_top_k.set(value.max(1));
                                        }
                                    }
                                    style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                                />
                                <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"How many past messages retrieval pulls back in per generation."</div>
                            </div>

                            <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                                <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Retrieval Score Threshold"</label>
                                <input
                                    type="number"
                                    min="0"
                                    max="1"
                                    step="0.05"
                                    prop:value=move || rag_score_threshold.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(value) = event_target_value(&ev).parse::<f64>() {
                                            set_rag_score_threshold.set(value.clamp(0.0, 1.0));
                                        }
                                    }
                                    style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                                />
                                <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"Cosine similarity floor (0-1) a match has to clear to count."</div>
                            </div>
                        </div>
                    })
                }}

                <div style="border-top: 1px solid var(--color-border); padding-top: 2rem;">
                    <h2 style="font-family: var(--font-heading); font-size: 1.4rem; font-weight: 400; margin: 0; color: #fff;">"Sampling Parameters"</h2>
                    <p style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; margin-top: 0.4rem; opacity: 0.8;">"Sent with every generation request to the main model above."</p>
                </div>

                <div style="display: flex; gap: 2rem; flex-wrap: wrap;">
                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Temperature"</label>
                        <input
                            type="number"
                            min="0"
                            step="0.05"
                            prop:value=move || temperature.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<f64>() {
                                    set_temperature.set(value.max(0.0));
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Top P"</label>
                        <input
                            type="number"
                            min="0"
                            max="1"
                            step="0.05"
                            prop:value=move || top_p.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<f64>() {
                                    set_top_p.set(value.clamp(0.0, 1.0));
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Top K"</label>
                        <input
                            type="number"
                            min="0"
                            step="1"
                            prop:value=move || top_k.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<i64>() {
                                    set_top_k.set(value.max(0));
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"0 = not sent (provider default)."</div>
                    </div>
                </div>

                <div style="display: flex; gap: 2rem; flex-wrap: wrap;">
                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Frequency Penalty"</label>
                        <input
                            type="number"
                            step="0.05"
                            prop:value=move || frequency_penalty.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<f64>() {
                                    set_frequency_penalty.set(value);
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Presence Penalty"</label>
                        <input
                            type="number"
                            step="0.05"
                            prop:value=move || presence_penalty.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<f64>() {
                                    set_presence_penalty.set(value);
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Max Response Tokens"</label>
                        <input
                            type="number"
                            min="0"
                            step="64"
                            prop:value=move || max_response_tokens.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<i64>() {
                                    set_max_response_tokens.set(value.max(0));
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"0 = provider default (usually 8192)."</div>
                    </div>

                    <div class="field" style="margin: 0; flex: 1; min-width: 200px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Reasoning Effort"</label>
                        <div style="position: relative; width: 100%;">
                            <select
                                prop:value=reasoning_effort
                                on:change=move |ev| set_reasoning_effort.set(event_target_value(&ev))
                                style="appearance: none; background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.1); border-radius: 8px; color: #fff; font-family: var(--font-body); padding: 0.75rem 1rem; font-size: 1rem; outline: none; width: 100%; transition: all 0.3s ease; cursor: pointer; box-shadow: inset 0 2px 4px rgba(0,0,0,0.2);"
                            >
                                <option value="" style="background: #1a1a1a; color: #fff;">"Provider default"</option>
                                <option value="low" style="background: #1a1a1a; color: #fff;">"Low"</option>
                                <option value="medium" style="background: #1a1a1a; color: #fff;">"Medium"</option>
                                <option value="high" style="background: #1a1a1a; color: #fff;">"High"</option>
                            </select>
                            <div style="position: absolute; right: 1rem; top: 50%; transform: translateY(-50%); pointer-events: none; color: var(--color-text-muted);">
                                <svg width="12" height="8" viewBox="0 0 12 8" fill="none" xmlns="http://www.w3.org/2000/svg">
                                    <path d="M1 1.5L6 6.5L11 1.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
                                </svg>
                            </div>
                        </div>
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"For reasoning models only. Caps how much of the response budget goes to <think> reasoning before the actual reply, so it doesn't eat the whole thing. Ignored by models that don't support it."</div>
                    </div>
                </div>

                <div style="display: flex; gap: 2rem; border-top: 1px solid var(--color-border); padding-top: 2rem; flex-wrap: wrap;">
                    <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Context Limit (Tokens)"</label>
                        <input
                            type="number"
                            min="0"
                            step="256"
                            prop:value=move || context_limit.get().to_string()
                            on:input=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<i64>() {
                                    set_context_limit.set(value.max(0));
                                }
                            }
                            style="background: transparent; border: none; border-bottom: 1px solid var(--color-border); color: #fff; font-family: monospace; padding: 0.5rem 0; font-size: 1rem; outline: none; width: 100%; transition: border-color 0.2s;"
                        />
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7;">"0 disables the limit."</div>
                    </div>
                    
                    <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                        <label class="checkbox-field" style="display: flex; align-items: center; gap: 0.5rem; margin-top: 2rem; color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; cursor: pointer;">
                            <input
                                type="checkbox"
                                prop:checked=forbid_external_media
                                on:change=move |ev| set_forbid_external_media.set(event_target_checked(&ev))
                                style="accent-color: #fff;"
                            />
                            "FORBID EXTERNAL MEDIA"
                        </label>
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; margin-top: 0.5rem; opacity: 0.7; margin-left: 1.5rem;">"Hide images rendered from markdown."</div>
                    </div>
                    
                    <div class="field" style="margin: 0; flex: 1; min-width: 250px;">
                        <label style="color: var(--color-text-muted); font-family: monospace; font-size: 0.85rem; text-transform: uppercase; letter-spacing: 0.05em; display: block; margin-bottom: 0.5rem;">"Text Speed: " {move || SPEED_PRESETS[speed_index.get()].1}</label>
                        <div style="padding-top: 0.5rem;">
                            <input
                                type="range"
                                min="0"
                                max=(SPEED_PRESETS.len() - 1).to_string()
                                step="1"
                                prop:value=move || speed_index.get().to_string()
                                on:input=on_speed_change
                                style="width: 100%; accent-color: #fff;"
                            />
                            <div style="display: flex; justify-content: space-between; color: var(--color-text-muted); font-family: monospace; font-size: 0.7rem; margin-top: 0.25rem;">
                                {SPEED_PRESETS.iter().map(|(_, label)| view! { <span>{*label}</span> }).collect_view()}
                            </div>
                        </div>
                    </div>
                </div>
                
                <div class="settings-actions" style="margin-top: 2rem; display: flex; align-items: center; gap: 1rem;">
                    <button type="submit" style="background: #fff; color: #000; border: none; padding: 0.75rem 2rem; font-family: monospace; font-weight: bold; text-transform: uppercase; font-size: 0.9rem; cursor: pointer; letter-spacing: 0.05em;">"Save Settings"</button>
                    {move || saved.get().then(|| view! { <span style="color: #4CAF50; font-family: monospace; font-size: 0.9rem;">"✓ SAVED"</span> })}
                </div>
                
                {move || error.get().map(|e| view! { <p style="color: #ff4444; font-family: monospace; border-left: 2px solid #ff4444; padding-left: 1rem; margin-top: 1rem;">{e}</p> })}
            </form>

            <div style="margin-top: 3rem; padding-top: 2rem; border-top: 1px solid var(--color-error);">
                <h3 style="color: var(--color-error); font-family: monospace; text-transform: uppercase; font-size: 0.9rem; margin-bottom: 1rem;">"Danger Zone"</h3>
                <div style="display: flex; align-items: center; gap: 1.5rem; margin-bottom: 1rem; flex-wrap: wrap;">
                    <button type="button" on:click=on_export_account style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.6rem 1.25rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; letter-spacing: 0.05em;">
                        "Export My Data"
                    </button>
                    <label style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text); padding: 0.6rem 1.25rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; letter-spacing: 0.05em;">
                        "Import Data"
                        <input type="file" accept="application/json" style="display: none;" on:change=on_import_account_file_selected />
                    </label>
                    {move || account_import_done.get().then(|| view! { <span style="color: #4CAF50; font-family: monospace; font-size: 0.8rem;">"✓ IMPORTED"</span> })}
                    {move || account_export_import_error.get().map(|e| view! { <span style="color: #ff4444; font-family: monospace; font-size: 0.8rem;">{e}</span> })}
                </div>
                <span style="color: var(--color-text-muted); font-family: monospace; font-size: 0.75rem; opacity: 0.7; display: block; margin-bottom: 1.5rem;">
                    "Exports characters, chats, groups, lorebooks, presets, regex scripts, and themes. Doesn't include your API keys or account settings. Import always adds new content, never overwrites."
                </span>
                <button
                    type="button"
                    on:click=move |_| set_delete_confirm_open.set(true)
                    style="background: transparent; border: 1px solid var(--color-error); color: var(--color-error); padding: 0.6rem 1.25rem; font-family: monospace; text-transform: uppercase; font-size: 0.8rem; cursor: pointer; letter-spacing: 0.05em;"
                >
                    "Delete Everything"
                </button>
            </div>
        </div>
    }
}
