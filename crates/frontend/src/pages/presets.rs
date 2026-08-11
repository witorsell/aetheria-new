use crate::api;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

#[component]
pub fn PresetsPage() -> impl IntoView {
    let version = RwSignal::new(0);
    let presets = LocalResource::new(move || {
        version.track();
        async move { api::list_presets().await.unwrap_or_default() }
    });
    let regex_scripts = LocalResource::new(move || {
        version.track();
        async move { api::list_regex_scripts().await.unwrap_or_default() }
    });
    let settings = LocalResource::new(move || {
        version.track();
        async move { api::get_settings().await.ok() }
    });

    let (error, set_error) = signal(Option::<String>::None);

    let preset_file_input = NodeRef::<leptos::html::Input>::new();
    let on_import_preset = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let files = input.files().unwrap();
        if let Some(file) = files.item(0) {
            spawn_local(async move {
                match api::import_preset(file).await {
                    Ok(_) => version.update(|v| *v += 1),
                    Err(e) => set_error.set(Some(format!("Failed to import preset: {e}"))),
                }
            });
        }
    };

    let script_file_input = NodeRef::<leptos::html::Input>::new();
    let on_import_script = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let files = input.files().unwrap();
        if let Some(file) = files.item(0) {
            spawn_local(async move {
                match api::import_regex_scripts(file).await {
                    Ok(_) => version.update(|v| *v += 1),
                    Err(e) => set_error.set(Some(format!("Failed to import regex script: {e}"))),
                }
            });
        }
    };

    let on_export_scripts = move |_| {
        spawn_local(async move {
            match api::export_regex_scripts().await {
                Ok(json) => {
                    let _ = api::download_text_file("aetheria-regex-scripts.json", "application/json", &json);
                }
                Err(e) => set_error.set(Some(format!("Failed to export regex scripts: {e}"))),
            }
        });
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; color: var(--color-text-muted); letter-spacing: -0.02em;">"Presets."</h1>
                <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 1.5rem; max-width: 40rem;">
                    "Import a SillyTavern completion preset to replace how the system prompt is assembled, or a regex script to clean up what gets sent to the model. Both are optional; with no preset active aetheria uses its own built-in prompt assembly."
                </p>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || {
                        let active_id = settings.get().flatten().and_then(|s| s.active_preset_id.clone());
                        presets.get().map(|list: Vec<api::Preset>| {
                            let active_id = active_id.clone();
                            view! {
                                <div style="display: flex; flex-direction: column; gap: 1rem; margin-bottom: 2.5rem;">
                                    <div style="display: flex; align-items: center; gap: 0.5rem; padding: 1rem; border: 1px solid var(--color-border);">
                                        <div style="flex: 1;">
                                            <div style="font-size: 1.1rem; font-family: var(--font-heading);">"aetheria's own assembly"</div>
                                            <div style="color: var(--color-text-muted); font-size: 0.875rem;">"No preset active"</div>
                                        </div>
                                        {if active_id.is_none() {
                                            view! { <span style="font-size: 0.8rem; color: var(--color-text-muted);">"Active"</span> }.into_any()
                                        } else {
                                            let on_click = move |_| {
                                                spawn_local(async move {
                                                    let _ = api::activate_preset(None).await;
                                                    version.update(|v| *v += 1);
                                                });
                                            };
                                            view! { <button class="btn secondary" on:click=on_click>"Use this"</button> }.into_any()
                                        }}
                                    </div>
                                    {list.into_iter().map(|p| {
                                        let is_active = active_id.as_deref() == Some(p.id.as_str());
                                        let preset_id_for_activate = p.id.clone();
                                        let preset_id_for_delete = p.id.clone();
                                        let preset_id_for_export = p.id.clone();
                                        let preset_name_for_export = p.name.clone();
                                        view! {
                                            <div style="display: flex; align-items: center; gap: 0.5rem; padding: 1rem; border: 1px solid var(--color-border);">
                <div style="flex: 1;">
                                                    <a href=format!("/presets/{}/edit", p.id) style="text-decoration: none; color: inherit;">
                                                        <div style="font-size: 1.1rem; font-family: var(--font-heading);">{p.name.clone()}</div>
                                                        <div style="color: var(--color-text-muted); font-size: 0.875rem;">{format!("{} prompt slots, {} enabled", p.prompts.len(), p.prompt_order.iter().filter(|e| e.enabled).count())}</div>
                                                    </a>
                                                </div>
                                                <a href=format!("/presets/{}/edit", p.id) class="btn secondary" style="text-decoration: none;">"Edit"</a>
                                                {if is_active {
                                                    view! { <span style="font-size: 0.8rem; color: var(--color-text-muted);">"Active"</span> }.into_any()
                                                } else {
                                                    let on_click = move |_| {
                                                        let id = preset_id_for_activate.clone();
                                                        spawn_local(async move {
                                                            let _ = api::activate_preset(Some(&id)).await;
                                                            version.update(|v| *v += 1);
                                                        });
                                                    };
                                                    view! { <button class="btn secondary" on:click=on_click>"Use this"</button> }.into_any()
                                                }}
                                                <button class="btn secondary" on:click=move |_| {
                                                    let id = preset_id_for_export.clone();
                                                    let name = preset_name_for_export.clone();
                                                    spawn_local(async move {
                                                        match api::export_preset(&id).await {
                                                            Ok(json) => {
                                                                let filename = format!("{}.json", name.replace('/', "-"));
                                                                let _ = api::download_text_file(&filename, "application/json", &json);
                                                            }
                                                            Err(e) => set_error.set(Some(format!("Failed to export preset: {e}"))),
                                                        }
                                                    });
                                                }>"Export"</button>
                                                <button class="btn ghost" on:click=move |_| {
                                                    let id = preset_id_for_delete.clone();
                                                    spawn_local(async move {
                                                        let _ = api::delete_preset(&id).await;
                                                        version.update(|v| *v += 1);
                                                    });
                                                }>"Delete"</button>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }
                        })
                    }}
                </Suspense>

                <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 1rem;">
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 2rem; margin: 0;">"Regex scripts."</h2>
                    <button type="button" class="btn secondary" on:click=on_export_scripts>"Export All"</button>
                </div>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || regex_scripts.get().map(|list: Vec<api::RegexScript>| view! {
                        <div style="display: flex; flex-direction: column; gap: 1rem;">
                            {list.into_iter().map(|s| {
                                let id_for_toggle = s.id.clone();
                                let id_for_delete = s.id.clone();
                                let currently_disabled = s.disabled;
                                view! {
                                    <div style="display: flex; align-items: center; gap: 0.5rem; padding: 1rem; border: 1px solid var(--color-border);">
                                        <div style="flex: 1;">
                                            <div style="font-size: 1.1rem; font-family: var(--font-heading);">{s.script_name.clone()}</div>
                                            <div style="color: var(--color-text-muted); font-size: 0.8rem; font-family: monospace; word-break: break-all;">{s.find_regex.clone()}</div>
                                        </div>
                                        <button class="btn secondary" on:click=move |_| {
                                            let id = id_for_toggle.clone();
                                            let next = !currently_disabled;
                                            spawn_local(async move {
                                                let _ = api::set_regex_script_disabled(&id, next).await;
                                                version.update(|v| *v += 1);
                                            });
                                        }>{if s.disabled { "Enable" } else { "Disable" }}</button>
                                        <button class="btn ghost" on:click=move |_| {
                                            let id = id_for_delete.clone();
                                            spawn_local(async move {
                                                let _ = api::delete_regex_script(&id).await;
                                                version.update(|v| *v += 1);
                                            });
                                        }>"Delete"</button>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </Suspense>
            </div>

            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Import."</h1>
                {move || error.get().map(|e| view! { <p class="error" style="margin-bottom: 1.5rem;">{e}</p> })}

                <div style="margin-bottom: 2.5rem;">
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 1.5rem; margin-bottom: 1rem;">"Completion preset"</h2>
                    <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 1rem;">"A SillyTavern completion preset export (.json)."</p>
                    <input type="file" accept=".json" on:change=on_import_preset node_ref=preset_file_input style="display: none;" />
                    <button type="button" class="btn secondary" on:click=move |_| { if let Some(el) = preset_file_input.get() { el.click(); } }>
                        "Import Preset"
                    </button>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 1.5rem; margin-bottom: 1rem;">"Regex script"</h2>
                    <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 1rem;">"A SillyTavern regex script export (.json), one script or an array of them."</p>
                    <input type="file" accept=".json" on:change=on_import_script node_ref=script_file_input style="display: none;" />
                    <button type="button" class="btn secondary" on:click=move |_| { if let Some(el) = script_file_input.get() { el.click(); } }>
                        "Import Regex Script"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn PresetEditorPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default().to_string());

    let version = RwSignal::new(0);
    let preset = LocalResource::new(move || {
        let current_id = id();
        version.track();
        async move { api::get_preset(&current_id).await.ok() }
    });

    // resource reloads.
    let entries = RwSignal::new(Vec::<(api::PresetOrderEntry, String)>::new());
    Effect::new(move |_| {
        if let Some(Some(p)) = preset.get().map(|p| p) {
            let names: std::collections::HashMap<String, String> =
                p.prompts.iter().map(|pr| (pr.identifier.clone(), pr.name.clone())).collect();
            let list = p.prompt_order.iter().map(|e| {
                let name = names.get(&e.identifier).cloned().unwrap_or_else(|| e.identifier.clone());
                (e.clone(), name)
            }).collect::<Vec<_>>();
            entries.set(list);
        }
    });

    let (saved, set_saved) = signal(false);

    let on_save = move |_| {
        let current_id = id();
        let order: Vec<api::PresetOrderEntry> = entries.get_untracked().into_iter().map(|(e, _)| e).collect();
        set_saved.set(false);
        spawn_local(async move {
            if api::update_preset_order(&current_id, order).await.is_ok() {
                set_saved.set(true);
            }
        });
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 0.5rem; color: var(--color-text-muted); letter-spacing: -0.02em;">
                    {move || preset.get().flatten().map(|p| p.name).unwrap_or_default()}
                </h1>
                <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 2rem;">
                    "Toggle which prompt slots are active, same as SillyTavern's own checklist. Order stays as imported."
                </p>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || {
                        preset.get().map(|_| view! {
                            <div style="display: flex; flex-direction: column; gap: 0.5rem;">
                                <For
                                    each=move || { let v: Vec<(usize, (api::PresetOrderEntry, String))> = entries.get().into_iter().enumerate().collect(); v }
                                    key=|(i, _)| *i
                                    children=move |(i, (entry, name))| {
                                        let checked = entry.enabled;
                                        view! {
                                            <label style="display: flex; align-items: center; gap: 0.75rem; padding: 0.6rem 0.75rem; border: 1px solid var(--color-border); cursor: pointer;">
                                                <input
                                                    type="checkbox"
                                                    prop:checked=checked
                                                    on:change=move |ev| {
                                                        let checked = event_target_checked(&ev);
                                                        entries.update(|list| {
                                                            if let Some((e, _)) = list.get_mut(i) {
                                                                e.enabled = checked;
                                                            }
                                                        });
                                                        set_saved.set(false);
                                                    }
                                                />
                                                <span>{name}</span>
                                            </label>
                                        }
                                    }
                                />
                            </div>
                        })
                    }}
                </Suspense>
            </div>

            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Save."</h1>
                <button type="button" class="btn primary" on:click=on_save>"Save Changes"</button>
                {move || saved.get().then(|| view! { <p style="margin-top: 1rem; color: var(--color-text-muted);">"Saved."</p> })}
                <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-top: 2rem;">
                    "Unchecked slots (like the README, or NSFW/POV variants you're not using) are skipped entirely when this preset builds a prompt."
                </p>
            </div>
        </div>
    }
}
