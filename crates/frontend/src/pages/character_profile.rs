use crate::api::{self, Character};
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;
use std::sync::Arc;

type NavFn = Arc<dyn Fn(&str, NavigateOptions) + Send + Sync + 'static>;

#[component]
pub fn CharacterProfilePage() -> impl IntoView {
    let params = use_params_map();
    let raw_navigate = use_navigate();
    let nav: NavFn = Arc::new(move |path: &str, opts: NavigateOptions| {
        raw_navigate(path, opts);
    });

    let id = move || params.read().get("id").unwrap_or_default();

    let character: LocalResource<Option<Character>> = LocalResource::new(move || {
        let id = id();
        async move { api::get_character(&id).await.ok() }
    });

    let greetings: LocalResource<Vec<crate::api::AlternateGreeting>> = LocalResource::new(move || {
        let id = id();
        async move { crate::api::list_alternate_greetings(&id).await.unwrap_or_default() }
    });

    let tags: LocalResource<Vec<crate::api::Tag>> = LocalResource::new(move || {
        let id = id();
        async move {
            let tag_ids = crate::api::get_character_tags(&id).await.unwrap_or_default();
            if tag_ids.is_empty() {
                return Vec::new();
            }
            let all = crate::api::list_tags().await.unwrap_or_default();
            all.into_iter().filter(|t| tag_ids.contains(&t.id)).collect()
        }
    });

    view! {
        <div class="dossier-page">
            <Suspense fallback=|| view! { <div class="dossier-loading">"Loading..."</div> }>
                {move || {
                    let nav = nav.clone();
                    let char_opt = character.get().flatten();
                    let greets = greetings.get().unwrap_or_default();
                    let char_tags = tags.get().unwrap_or_default();
                    char_opt.map(move |char| {
                        view! { <CharacterDossier char=char nav=nav.clone() greetings=greets.clone() tags=char_tags.clone() /> }
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn CharacterDossier(char: Character, nav: NavFn, greetings: Vec<crate::api::AlternateGreeting>, tags: Vec<crate::api::Tag>) -> impl IntoView {
    let file_no = char.id
        .replace('-', "")
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>()
        .to_uppercase();

    let char_id_chats = char.id.clone();
    let char_id_edit  = char.id.clone();
    let char_id_new   = char.id.clone();
    let char_id_export = char.id.clone();
    let nav_chats = nav.clone();
    let nav_edit  = nav.clone();
    let nav_back  = nav.clone();
    let nav_new   = nav.clone();

    let avatar_url  = char.avatar_url.clone();
    let initial     = char.name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let char_name   = char.name.clone();
    let char_name_stored = StoredValue::new(char.name.clone());
    let description   = StoredValue::new(char.description.clone());
    let personality   = StoredValue::new(char.personality.clone());
    let scenario      = StoredValue::new(char.scenario.clone());
    let first_message = StoredValue::new(char.first_message.clone());

    let has_description = !char.description.is_empty();
    let has_personality = !char.personality.is_empty();
    let has_scenario    = !char.scenario.is_empty();
    let has_greeting    = !char.first_message.is_empty();

    let settings = LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);

    let me = LocalResource::new(|| async move { crate::api::fetch_me().await.ok() });
    let user_name = move || me.get().flatten().and_then(|m| m.display_name).unwrap_or_else(|| me.get().flatten().map(|m| m.username).unwrap_or_else(|| "User".to_string()));

    view! {
        <div>
            <nav class="dossier-nav">
                <button class="ghost" on:click=move |_| {
                    nav_back("/characters", NavigateOptions::default());
                }>"← Characters"</button>
                <span class="dossier-nav-sep">"/ Character File"</span>
            </nav>

            <div class="dossier">
                <aside class="dossier-rail">
                    <div class="dossier-portrait">
                        {move || {
                            if let Some(url) = avatar_url.clone().filter(|_| !forbid_media()) {
                                view! { <img src=url alt="avatar" /> }.into_any()
                            } else {
                                view! { <div class="dossier-portrait-fallback">{initial.clone()}</div> }.into_any()
                            }
                        }}
                    </div>

                    <div class="dossier-fileno">
                        "FILE № " <b>{file_no}</b>
                    </div>

                    <div class="dossier-rail-actions" style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.25rem;">
                        <button class="primary" on:click=move |_| {
                            let nav = nav_new.clone();
                            let id = char_id_new.clone();
                            leptos::task::spawn_local(async move {
                                if let Ok(chat) = api::create_chat(&id, "Chat").await {
                                    nav(&format!("/chat/{}", chat.id), NavigateOptions::default());
                                }
                            });
                        }>
                            "Start Chat"
                        </button>
                        <button class="ghost" on:click=move |_| {
                            nav_chats(&format!("/characters/{}/chats", char_id_chats), NavigateOptions::default());
                        }>
                            "History"
                        </button>
                        <button class="ghost" on:click=move |_| {
                            nav_edit(&format!("/characters/{}/edit", char_id_edit), NavigateOptions::default());
                        }>
                            "Edit"
                        </button>
                        <a href=format!("/api/export/character/{}?format=json", char_id_export.clone()) download="character.json" class="btn ghost">"Export JSON"</a>
                        <a href=format!("/api/export/character/{}?format=png", char_id_export.clone()) download="character.png" class="btn ghost">"Export PNG"</a>

                    </div>
                </aside>

                <div class="dossier-body">
                    <h1 class="dossier-name">{char_name}</h1>

                    {if tags.is_empty() {
                        view! {}.into_any()
                    } else {
                        view! {
                            <div style="display: flex; flex-wrap: wrap; gap: 0.4rem; margin: -0.5rem 0 1.5rem;">
                                {tags.into_iter().map(|t| view! {
                                    <span style=format!(
                                        "display: inline-flex; align-items: center; gap: 0.35rem; padding: 0.2rem 0.6rem; border-radius: 999px; border: 1px solid {}; font-size: 0.8rem; color: var(--color-text-muted);",
                                        t.color
                                    )>
                                        <span style=format!("width: 0.5rem; height: 0.5rem; border-radius: 50%; background: {};", t.color)></span>
                                        {t.name}
                                    </span>
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}

                    <div class="dossier-content">
                        {if has_description {
                            view! {
                                <DossierSection title="About">
                                    <div class="rendered-markdown">{move || crate::render::markdown::render_markdown(&description.get_value(), &char_name_stored.get_value(), &user_name(), forbid_media())}</div>
                                </DossierSection>
                            }.into_any()
                        } else { view! {}.into_any() }}

                        {if has_personality {
                            view! {
                                <DossierSection title="Persona">
                                    <div class="rendered-markdown">{move || crate::render::markdown::render_markdown(&personality.get_value(), &char_name_stored.get_value(), &user_name(), forbid_media())}</div>
                                </DossierSection>
                            }.into_any()
                        } else { view! {}.into_any() }}

                        {if has_scenario {
                            view! {
                                <DossierSection title="Scenario">
                                    <div class="rendered-markdown">{move || crate::render::markdown::render_markdown(&scenario.get_value(), &char_name_stored.get_value(), &user_name(), forbid_media())}</div>
                                </DossierSection>
                            }.into_any()
                        } else { view! {}.into_any() }}

                        {if has_greeting {
                            view! {
                                <DossierSection title="First Meeting">
                                    <div class="rendered-markdown">{move || crate::render::markdown::render_markdown(&first_message.get_value(), &char_name_stored.get_value(), &user_name(), forbid_media())}</div>
                                </DossierSection>
                            }.into_any()
                        } else { view! {}.into_any() }}
                    </div>

                        {if !greetings.is_empty() {
                            let greets: Vec<(usize, crate::api::AlternateGreeting)> = greetings.clone().into_iter().enumerate().collect();
                            let greets_stored = StoredValue::new(greets);
                            let char_name_alt = char_name_stored.clone();
                            view! {
                                <DossierSection title="Alternate Greetings">
                                    <div class="dossier-altgreetings">
                                        <For each=move || greets_stored.get_value() key=|(i, g)| g.id.clone() let:greet_tuple>
                                            <AltGreeting index=greet_tuple.0 html=greet_tuple.1.greeting char_name=char_name_alt.get_value() user_name=user_name() forbid_media=forbid_media() />
                                        </For>
                                    </div>
                                </DossierSection>
                            }.into_any()
                        } else { view! {}.into_any() }}

                </div>
            </div>
        </div>
    }
}

#[component]
fn DossierSection(title: &'static str, children: ChildrenFn) -> impl IntoView {
    let (open, set_open) = signal(false);

    view! {
        <div class="dossier-section" class:dossier-section-open=move || open.get()>
            <h2
                role="button"
                tabindex="0"
                on:click=move |_| set_open.update(|v| *v = !*v)
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Enter" || ev.key() == " " {
                        ev.prevent_default();
                        set_open.update(|v| *v = !*v);
                    }
                }
            >
                {title}
                <span class="dossier-section-chevron" class:open=move || open.get()>"›"</span>
            </h2>
            <Show when=move || open.get()>
                <div class="dossier-section-body">
                    {children()}
                </div>
            </Show>
        </div>
    }
}


#[component]
fn AltGreeting(index: usize, html: String, char_name: String, user_name: String, forbid_media: bool) -> impl IntoView {
    let (open, set_open) = signal(false);
    let html_stored = StoredValue::new(html);
    let char_name_stored = StoredValue::new(char_name);
    
    view! {
        <div class="dossier-altgreeting" class:dossier-altgreeting-open=move || open.get()>
            <div
                class="dossier-altgreeting-label"
                role="button"
                tabindex="0"
                on:click=move |_| set_open.update(|v| *v = !*v)
                on:keydown=move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Enter" || ev.key() == " " {
                        ev.prevent_default();
                        set_open.update(|v| *v = !*v);
                    }
                }
            >
                {format!("Alternate Greeting {}", index + 1)}
                <span class="dossier-section-chevron" class:open=move || open.get()>"›"</span>
            </div>
            <Show when=move || open.get()>
                <div class="rendered-markdown">
                    {crate::render::markdown::render_markdown(&html_stored.get_value(), &char_name_stored.get_value(), &user_name, forbid_media)}
                </div>
            </Show>
        </div>
    }
}
