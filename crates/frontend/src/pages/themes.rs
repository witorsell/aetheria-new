use crate::api;
use crate::theme::ThemeStore;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

#[component]
pub fn ThemesPage() -> impl IntoView {
    let version = RwSignal::new(0);
    let themes = LocalResource::new(move || {
        version.track();
        async move { api::list_themes().await.unwrap_or_default() }
    });
    let theme_store = use_context::<ThemeStore>();
    let (error, set_error) = signal(Option::<String>::None);
    let navigate = use_navigate();

    let own_file_input = NodeRef::<leptos::html::Input>::new();
    let on_import_own = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        if let Some(file) = input.files().and_then(|f| f.item(0)) {
            spawn_local(async move {
                match api::import_theme(file).await {
                    Ok((_, warning)) => {
                        version.update(|v| *v += 1);
                        if let Some(w) = warning {
                            set_error.set(Some(w));
                        }
                    }
                    Err(e) => set_error.set(Some(format!("Failed to import theme: {e}"))),
                }
            });
        }
    };

    let st_file_input = NodeRef::<leptos::html::Input>::new();
    let on_import_st = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        if let Some(file) = input.files().and_then(|f| f.item(0)) {
            spawn_local(async move {
                match api::import_st_theme(file).await {
                    Ok((_, warning)) => {
                        version.update(|v| *v += 1);
                        if let Some(w) = warning {
                            set_error.set(Some(w));
                        }
                    }
                    Err(e) => set_error.set(Some(format!("Failed to import SillyTavern theme: {e}"))),
                }
            });
        }
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; color: var(--color-text-muted); letter-spacing: -0.02em;">"Themes."</h1>
                {move || error.get().map(|e| view! { <p class="error" style="margin-bottom: 1.5rem;">{e}</p> })}
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || themes.get().map(|list: Vec<api::ThemeListItem>| view! {
                        <div style="display: flex; flex-direction: column; gap: 1rem;">
                            {list.into_iter().map(|t| {
                                let id_for_activate = t.id.clone();
                                let id_for_delete = t.id.clone();
                                let id_for_export = t.id.clone();
                                let name_for_export = t.name.clone();
                                let name_for_duplicate = t.name.clone();
                                let tokens_for_duplicate = t.tokens.clone();
                                let navigate_for_duplicate = navigate.clone();
                                let edit_href = format!("/themes/{}/edit", t.id);
                                let builtin = t.builtin;
                                let is_active = t.active;
                                view! {
                                    <div style="display: flex; align-items: center; gap: 0.5rem; padding: 1rem; border: 1px solid var(--color-border); border-radius: var(--radius-md);">
                                        <div style="flex: 1;">
                                            <a href=edit_href.clone() style="text-decoration: none; color: inherit;">
                                                <div style="font-size: 1.1rem; font-family: var(--font-heading);">{t.name.clone()}</div>
                                                <div style="color: var(--color-text-muted); font-size: 0.875rem;">{if builtin { "Built-in" } else { "Custom" }}</div>
                                            </a>
                                        </div>
                                        {if !builtin {
                                            view! { <a href=edit_href class="btn secondary" style="text-decoration: none;">"Edit"</a> }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }}
                                        {if is_active {
                                            view! { <span style="font-size: 0.8rem; color: var(--color-text-muted);">"Active"</span> }.into_any()
                                        } else {
                                            let on_click = move |_| {
                                                let id = id_for_activate.clone();
                                                spawn_local(async move {
                                                    if api::activate_theme(&id).await.is_ok() {
                                                        version.update(|v| *v += 1);
                                                        if let Some(store) = theme_store {
                                                            if let Ok(tokens) = api::get_active_theme().await {
                                                                store.0.set(tokens);
                                                            }
                                                        }
                                                    }
                                                });
                                            };
                                            view! { <button class="btn secondary" on:click=on_click>"Use this"</button> }.into_any()
                                        }}
                                        <button class="btn secondary" on:click=move |_| {
                                            let id = id_for_export.clone();
                                            let name = name_for_export.clone();
                                            spawn_local(async move {
                                                match api::export_theme(&id).await {
                                                    Ok(json) => { let _ = api::download_text_file(&format!("{}.json", name.replace('/', "-")), "application/json", &json); }
                                                    Err(e) => set_error.set(Some(format!("Failed to export theme: {e}"))),
                                                }
                                            });
                                        }>"Export"</button>
                                        <button class="btn secondary" on:click=move |_| {
                                            let name = format!("{} (copy)", name_for_duplicate.clone());
                                            let tokens = tokens_for_duplicate.clone();
                                            let navigate = navigate_for_duplicate.clone();
                                            spawn_local(async move {
                                                match api::create_theme(&name, &tokens).await {
                                                    Ok(new_theme) => {
                                                        version.update(|v| *v += 1);
                                                        navigate(&format!("/themes/{}/edit", new_theme.id), Default::default());
                                                    }
                                                    Err(e) => set_error.set(Some(format!("Failed to duplicate theme: {e}"))),
                                                }
                                            });
                                        }>"Duplicate"</button>
                                        {if !builtin {
                                            view! {
                                                <button class="btn ghost" on:click=move |_| {
                                                    let id = id_for_delete.clone();
                                                    spawn_local(async move {
                                                        let _ = api::delete_theme(&id).await;
                                                        version.update(|v| *v += 1);
                                                    });
                                                }>"Delete"</button>
                                            }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }}
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </Suspense>
            </div>
            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Import."</h1>
                <div style="margin-bottom: 2rem;">
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 1.5rem; margin-bottom: 1rem;">"Aetheria theme"</h2>
                    <input type="file" accept=".json" on:change=on_import_own node_ref=own_file_input style="display: none;" />
                    <button type="button" class="btn secondary" on:click=move |_| { if let Some(el) = own_file_input.get() { el.click(); } }>"Import Theme"</button>
                </div>
                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 1.5rem; margin-bottom: 1rem;">"SillyTavern theme"</h2>
                    <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 1rem;">"A SillyTavern UI theme export (.json). Fields aetheria doesn't have (like the mascot) fall back to defaults."</p>
                    <input type="file" accept=".json" on:change=on_import_st node_ref=st_file_input style="display: none;" />
                    <button type="button" class="btn secondary" on:click=move |_| { if let Some(el) = st_file_input.get() { el.click(); } }>"Import SillyTavern Theme"</button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn ThemeEditorPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default().to_string());
    let theme_store = use_context::<ThemeStore>();

    let theme = LocalResource::new(move || {
        let current_id = id();
        async move { api::get_theme(&current_id).await.ok() }
    });

    let name = RwSignal::new(String::new());
    let tokens = RwSignal::new(api::ThemeTokens::default());
    Effect::new(move |_| {
        if let Some(Some(t)) = theme.get().map(|t| t) {
            name.set(t.name);
            tokens.set(t.tokens);
        }
    });

    // live preview: every edit is applied immediately, same as SillyTavern's
    // color pickers/sliders. saving persists it; navigating away without
    // saving leaves the *stored* theme untouched, the next activation will
    // reload the saved tokens, not these in-progress ones.
    Effect::new(move |_| {
        crate::theme::apply_tokens_to_root(&tokens.get());
    });

    // an edit that's never saved shouldn't outlive the page it was made on:
    // restore the actually-active theme's tokens when the editor unmounts,
    // so navigating away without saving doesn't leave the preview stuck.
    on_cleanup(move || {
        if let Some(store) = theme_store {
            crate::theme::apply_tokens_to_root(&store.0.get_untracked());
        }
    });

    let (saved, set_saved) = signal(false);
    let on_save = move |_| {
        let current_id = id();
        let n = name.get_untracked();
        let t = tokens.get_untracked();
        set_saved.set(false);
        spawn_local(async move {
            if api::update_theme(&current_id, &n, &t).await.is_ok() {
                set_saved.set(true);
            }
        });
    };

    macro_rules! color_field {
        ($label:expr, $field:ident) => {
            view! {
                <div class="field">
                    <label>{$label}</label>
                    <input type="text" prop:value=move || tokens.get().$field
                        on:input=move |ev| tokens.update(|t| t.$field = event_target_value(&ev)) />
                </div>
            }
        };
    }

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 0.5rem; letter-spacing: -0.02em;">
                    <input type="text" prop:value=name on:input=move |ev| name.set(event_target_value(&ev))
                        style="background: transparent; border: none; color: inherit; font: inherit; width: 100%;" />
                </h1>
                <div style="display: flex; flex-direction: column; gap: 1rem; max-width: 32rem;">
                    {color_field!("Background", color_bg)}
                    {color_field!("Surface", color_surface)}
                    {color_field!("Border", color_border)}
                    {color_field!("Accent", color_accent)}
                    {color_field!("Accent 2", color_accent_2)}
                    {color_field!("Text", color_text)}
                    {color_field!("Muted text", color_text_muted)}
                    {color_field!("Error", color_error)}
                    {color_field!("Mascot accent", mascot_accent)}

                    <div class="field">
                        <label>"Font scale"</label>
                        <input type="range" min="0.8" max="1.3" step="0.05"
                            prop:value=move || tokens.get().font_scale.to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    tokens.update(|t| t.font_scale = v);
                                }
                            } />
                    </div>
                    <div class="field">
                        <label>"Blur strength"</label>
                        <input type="range" min="0" max="20" step="1"
                            prop:value=move || tokens.get().blur_strength.to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    tokens.update(|t| t.blur_strength = v);
                                }
                            } />
                    </div>
                    <div class="checkbox-field">
                        <input type="checkbox" prop:checked=move || tokens.get().mascot_enabled
                            on:change=move |ev| { let c = event_target_checked(&ev); tokens.update(|t| t.mascot_enabled = c); } />
                        <label>"Show Aeth (the mascot)"</label>
                    </div>
                    <div class="field">
                        <label>"Custom CSS"</label>
                        <textarea style="min-height: 160px; font-family: monospace;"
                            prop:value=move || tokens.get().custom_css
                            on:input=move |ev| { let v = event_target_value(&ev); tokens.update(|t| t.custom_css = v); }></textarea>
                    </div>
                </div>
            </div>
            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Save."</h1>
                <p style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 1.5rem;">"Every change previews instantly across the whole app. Nothing is written until you save."</p>
                <button type="button" class="btn primary" on:click=on_save>"Save Changes"</button>
                {move || saved.get().then(|| view! { <p style="margin-top: 1rem; color: var(--color-text-muted);">"Saved."</p> })}
            </div>
        </div>
    }
}
