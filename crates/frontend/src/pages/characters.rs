use crate::api::{self, CharacterInput};
use crate::components::character_list::{CharacterList, CharactersVersion};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn CharactersPage() -> impl IntoView {
    let characters_version = RwSignal::new(0);
    provide_context(CharactersVersion(characters_version));

    let (selected_tag, set_selected_tag) = signal(Option::<String>::None);
    let all_tags = LocalResource::new(move || {
        characters_version.get();
        async move { api::list_tags().await.unwrap_or_default() }
    });

    let (name, set_name) = signal(String::new());
    let (description, set_description) = signal(String::new());
    let (first_message, set_first_message) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (importing, set_importing) = signal(false);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        
        let n = name.get_untracked();
        let d = description.get_untracked();
        let fm = first_message.get_untracked();
        
        if n.trim().is_empty() || d.trim().is_empty() || fm.trim().is_empty() {
            set_error.set(Some("You cannot draft a soul from nothingness. Provide a moniker, essence, and first breath.".to_string()));
            return;
        }

        spawn_local(async move {
            let input = CharacterInput {
                name: &n,
                description: Some(&d),
                personality: None,
                scenario: None,
                first_message: Some(&fm),
                avatar_url: None,
                sample_chat: None,
                system_prompt: None,
                post_history_instructions: None,
                prefill: None,
                insert_depth_prompt: None,
                insert_depth: None,
                persona: None,
                extensions: None,
                folder_id: None,
                talkativeness: None,
            };
            match api::create_character(input).await {
                Ok(_) => {
                    set_name.set(String::new());
                    set_description.set(String::new());
                    set_first_message.set(String::new());
                    set_error.set(None);
                    characters_version.update(|v| *v += 1);
                }
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let on_import = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        if let Some(files) = input.files() {
            if let Some(file) = files.get(0_u32) {
                set_importing.set(true);
                set_error.set(None);
                spawn_local(async move {
                    let result = crate::api::import_character(file).await;
                    set_importing.set(false);
                    match result {
                        Ok(_) => {
                            set_error.set(None);
                            characters_version.update(|v| *v += 1);
                        }
                        Err(e) => set_error.set(Some(e)),
                    }
                });
            }
        }
    };

    view! {
        <div class="library-layout">
            <div class="library-pane">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem;">
                    <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; color: var(--color-text-muted); letter-spacing: -0.02em; margin: 0;">"Library."</h1>
                    <form method="POST" action="/api/logout">
                        <button type="submit" class="btn" style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text-muted);">"Logout"</button>
                    </form>
                </div>
                <div style="display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 1rem;">
                    <button
                        class="ghost small"
                        style=move || if selected_tag.get().is_none() { "border-color: var(--color-accent);".to_string() } else { String::new() }
                        on:click=move |_| set_selected_tag.set(None)
                    >"All"</button>
                    {move || all_tags.get().unwrap_or_default().into_iter().map(|t| {
                        let id = t.id.clone();
                        let id2 = t.id.clone();
                        view! {
                            <button
                                class="ghost small"
                                style=move || {
                                    let base = format!("border-color: {}; display: inline-flex; align-items: center; gap: 0.35rem;", t.color);
                                    if selected_tag.get().as_deref() == Some(id.as_str()) { format!("{base} background: color-mix(in srgb, {} 25%, transparent); font-weight: 600;", t.color) } else { base }
                                }
                                on:click=move |_| set_selected_tag.set(Some(id2.clone()))
                            >
                                <span style=format!("width: 0.5rem; height: 0.5rem; border-radius: 50%; background: {};", t.color)></span>
                                {t.name.clone()}
                            </button>
                        }
                    }).collect::<Vec<_>>()}
                </div>
                <CharacterList filter_tag=Signal::derive(move || selected_tag.get()) />
            </div>
            
            <div class="library-draft">
                <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 3rem; margin-bottom: 2rem; letter-spacing: -0.02em;">"Draft a soul."</h1>
                <form on:submit=on_create style="display: flex; flex-direction: column; gap: 2rem;">
                    <div class="field">
                        <label>"Moniker"</label>
                        <input
                            type="text"
                            placeholder="A name to call them by..."
                            prop:value=name
                            on:input=move |ev| set_name.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="field">
                        <label>"Essence"</label>
                        <input
                            type="text"
                            placeholder="What are they?"
                            prop:value=description
                            on:input=move |ev| set_description.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="field">
                        <label>"First breath"</label>
                        <textarea
                            placeholder="The first words they will speak..."
                            prop:value=first_message
                            on:input=move |ev| set_first_message.set(event_target_value(&ev))
                            style="min-height: 100px; resize: vertical;"
                        ></textarea>
                    </div>
                    
                    <button type="submit" class="btn primary" style="align-self: flex-start; margin-top: 1rem;">"Inscribe"</button>
                </form>
                
                <div style="margin-top: 4rem; padding-top: 2rem; border-top: 1px solid var(--color-border);">
                    <div class="file-upload-wrapper">
                        <input type="file" id="import-upload" class="file-upload-input" accept=".png,.json" disabled=move || importing.get() on:change=on_import />
                        <label for="import-upload" class="file-upload-label">
                            {move || if importing.get() { "Resurrecting... this can take a moment for large cards" } else { "Resurrect from archives (PNG/JSON)" }}
                        </label>
                    </div>
                </div>
                
                {move || error.get().map(|e| view! { <p class="error" style="margin-top: 2rem;">{e}</p> })}
            </div>
        </div>
    }
}
