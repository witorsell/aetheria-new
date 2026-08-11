use crate::api::{self, CreateLorebookInput, CreateLorebookEntryInput};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use leptos::task::spawn_local;

#[component]
pub fn LorebooksPage() -> impl IntoView {
    let version = RwSignal::new(0);
    let lorebooks = LocalResource::new(
        move || {
            version.track();
            async move { api::list_lorebooks().await.unwrap_or_default() }
        }
    );

    let (name, set_name) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get_untracked();
        let d = description.get_untracked();
        if n.trim().is_empty() { return; }
        
        spawn_local(async move {
            let input = CreateLorebookInput {
                name: n,
                description: if d.trim().is_empty() { None } else { Some(d) },
                scan_depth: Some(2),
                token_budget: Some(1000),
                recursive_scanning: Some(false),
                extensions: None,
            };
            match api::create_lorebook(&input).await {
                Ok(_) => {
                    set_name.set(String::new());
                    set_description.set(String::new());
                    version.update(|v| *v += 1);
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let file_input = NodeRef::<leptos::html::Input>::new();
    let on_import = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let files = input.files().unwrap();
        if let Some(file) = files.item(0) {
            spawn_local(async move {
                let form_data = web_sys::FormData::new().unwrap();
                form_data.append_with_blob("file", &file).unwrap();
                let req = web_sys::RequestInit::new();
                req.set_method("POST");
                req.set_body(&form_data);
                let request = web_sys::Request::new_with_str_and_init("/api/import/lorebook", &req).unwrap();
                let window = web_sys::window().unwrap();
                match wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await {
                    Ok(resp_val) => {
                        use wasm_bindgen::JsCast;
                        let resp: web_sys::Response = resp_val.dyn_into().unwrap();
                        if resp.ok() {
                            version.update(|v| *v += 1);
                        } else {
                            set_error.set(Some("Failed to import lorebook".to_string()));
                        }
                    }
                    Err(_) => set_error.set(Some("Network error".to_string())),
                }
            });
        }
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; color: var(--color-text-muted); letter-spacing: -0.02em;">"Lorebooks."</h1>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || lorebooks.get().map(|list: Vec<crate::api::Lorebook>| view! {
                        <div style="display: flex; flex-direction: column; gap: 1rem;">
                            {list.into_iter().map(|l| view! {
                                <div style="display: flex; align-items: center; gap: 0.5rem; padding: 1rem; border: 1px solid var(--color-border);">
                                    <a href=format!("/lorebooks/{}/edit", l.id) style="flex: 1; text-decoration: none; color: inherit;">
                                        <div style="font-size: 1.25rem; font-family: var(--font-heading); margin-bottom: 0.5rem;">{l.name.clone()}</div>
                                        <div style="color: var(--color-text-muted); font-size: 0.875rem;">{l.description}</div>
                                    </a>
                                    <a href=format!("/api/export/lorebook/{}", l.id) download=format!("{}.json", l.name) class="ghost button" style="border: 1px solid var(--border-color); padding: 0.5rem; border-radius: 4px; text-decoration: none; white-space: nowrap;">"Export"</a>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </Suspense>
            </div>
            
            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Create a lorebook."</h1>
                <form on:submit=on_create style="display: flex; flex-direction: column; gap: 2rem;">
                    <div class="field">
                        <label>"Title"</label>
                        <input type="text" placeholder="Name of the lorebook..." prop:value=name on:input=move |ev| set_name.set(event_target_value(&ev)) />
                    </div>
                    <div class="field">
                        <label>"Purpose"</label>
                        <input type="text" placeholder="What secrets does it hold?" prop:value=description on:input=move |ev| set_description.set(event_target_value(&ev)) />
                    </div>
                    <button type="submit" class="btn primary" style="align-self: flex-start; margin-top: 1rem;">"Create"</button>
                </form>
                {move || error.get().map(|e| view! { <p class="error" style="margin-top: 2rem;">{e}</p> })}
                
                <div style="margin-top: 2rem; border-top: 1px solid var(--color-border); padding-top: 2rem;">
                    <h2 style="font-family: var(--font-heading); font-weight: 300; font-size: 2rem; margin-bottom: 1rem;">"Import Lorebook"</h2>
                    <input type="file" accept=".json,.png" on:change=on_import node_ref=file_input style="display: none;" />
                    <button type="button" class="btn secondary" on:click=move |_| { if let Some(el) = file_input.get() { el.click(); } }>
                        "Import File"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn LorebookEditorPage() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.with(|p| p.get("id").unwrap_or_default().to_string());
    
    let version = RwSignal::new(0);
    let entries = LocalResource::new(
        move || {
            let current_id = id();
            version.track();
            async move { api::list_lorebook_entries(&current_id).await.unwrap_or_default() }
        }
    );

    let (entry_name, set_entry_name) = signal(String::new());
    let (entry_text, set_entry_text) = signal(String::new());
    let (entry_keys, set_entry_keys) = signal(String::new());

    let on_add = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = entry_name.get_untracked();
        let txt = entry_text.get_untracked();
        let keys = entry_keys.get_untracked();
        let lorebook_id = id();
        
        if n.trim().is_empty() || txt.trim().is_empty() { return; }
        
        spawn_local(async move {
            let input = CreateLorebookEntryInput {
                lorebook_id,
                name: n,
                entry: txt,
                keywords: Some(keys),
                priority: Some(10),
                weight: Some(10),
                enabled: Some(true),
                comment: None,
                secondary_keys: None,
                constant: Some(false),
                position: None,
                probability: None,
                use_probability: None,
                selective: None,
                selective_logic: None,
                exclude_recursion: None,
            };
            match api::create_lorebook_entry(&input).await {
                Ok(_) => {
                    set_entry_name.set(String::new());
                    set_entry_text.set(String::new());
                    set_entry_keys.set(String::new());
                    version.update(|v| *v += 1);
                }
                Err(_) => {}
            }
        });
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; color: var(--color-text-muted); letter-spacing: -0.02em;">"Entries."</h1>
                <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                    {move || entries.get().map(|list: Vec<crate::api::LorebookEntry>| view! {
                        <div style="display: flex; flex-direction: column; gap: 1rem;">
                            {list.into_iter().map(|e| view! {
                                <div style="padding: 1rem; border: 1px solid var(--color-border);">
                                    <div style="font-size: 1.25rem; font-family: var(--font-heading); margin-bottom: 0.5rem;">{e.name}</div>
                                    <div style="color: var(--color-text-muted); font-size: 0.875rem; margin-bottom: 0.5rem;">"Keys: " {e.keywords}</div>
                                    <div style="font-size: 0.875rem; white-space: pre-wrap;">{e.entry}</div>
                                </div>
                            }).collect::<Vec<_>>()}
                        </div>
                    })}
                </Suspense>
            </div>
            
            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"New entry."</h1>
                <form on:submit=on_add style="display: flex; flex-direction: column; gap: 2rem;">
                    <div class="field">
                        <label>"Name"</label>
                        <input type="text" placeholder="Entry identifier..." prop:value=entry_name on:input=move |ev| set_entry_name.set(event_target_value(&ev)) />
                    </div>
                    <div class="field">
                        <label>"Keys (comma separated)"</label>
                        <input type="text" placeholder="apple, banana, fruit..." prop:value=entry_keys on:input=move |ev| set_entry_keys.set(event_target_value(&ev)) />
                    </div>
                    <div class="field">
                        <label>"Text"</label>
                        <textarea placeholder="The knowledge to inject..." prop:value=entry_text on:input=move |ev| set_entry_text.set(event_target_value(&ev)) style="min-height: 100px; resize: vertical;"></textarea>
                    </div>
                    <button type="submit" class="btn primary" style="align-self: flex-start; margin-top: 1rem;">"Add Entry"</button>
                </form>
            </div>
        </div>
    }
}
