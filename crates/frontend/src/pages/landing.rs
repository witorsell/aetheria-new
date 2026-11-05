use crate::api::{self, Character, RecentChat};
use leptos::prelude::*;
use leptos_router::components::A;

fn format_relative(ts_ms: i64) -> String {
    let now = js_sys::Date::now() as i64;
    let secs = ((now - ts_ms) / 1000).max(0);
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hrs = mins / 60;
    if hrs < 24 {
        return format!("{hrs}h ago");
    }
    let days = hrs / 24;
    if days < 30 {
        return format!("{days}d ago");
    }
    format!("{}mo ago", days / 30)
}

fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

#[component]
fn Avatar(name: String, url: Option<String>, forbid_media: bool) -> impl IntoView {
    let letter = initial(&name);
    view! {
        <div class="desk-avatar" aria-hidden="true">
            {if let Some(url) = url.filter(|_| !forbid_media) {
                view! { <img src=url alt="" /> }.into_any()
            } else {
                view! { <span>{letter}</span> }.into_any()
            }}
        </div>
    }
}

#[component]
fn ChatRow(chat: RecentChat, forbid_media: bool) -> impl IntoView {
    let who = chat
        .group_name
        .clone()
        .or_else(|| chat.character_name.clone())
        .unwrap_or_else(|| "Chat".to_string());
    let stored = chat.title.trim();
    let generic = stored.is_empty() || stored.eq_ignore_ascii_case("chat") || stored.eq_ignore_ascii_case("untitled");
    let title = if generic { who.clone() } else { stored.to_string() };
    let avatar_url = chat.group_avatar.clone().or_else(|| chat.character_avatar.clone());
    let when = format_relative(chat.updated_at);
    let detail = if title == who {
        when.clone()
    } else {
        format!("{who} · {when}")
    };
    let href = format!("/chat/{}", chat.id);
    view! {
        <A href=href attr:class="desk-row">
            <Avatar name=who url=avatar_url forbid_media=forbid_media />
            <span class="desk-row-text">
                <span class="desk-row-title">{title}</span>
                <span class="desk-row-detail">{detail}</span>
            </span>
        </A>
    }
}

#[component]
fn CharacterRow(character: Character, forbid_media: bool) -> impl IntoView {
    let href = format!("/characters/{}", character.id);
    let name = character.name.clone();
    view! {
        <A href=href attr:class="desk-row">
            <Avatar name=name.clone() url=character.avatar_url.clone() forbid_media=forbid_media />
            <span class="desk-row-text">
                <span class="desk-row-title">{name}</span>
            </span>
        </A>
    }
}

#[component]
pub fn LandingPage() -> impl IntoView {
    let settings = LocalResource::new(|| async move { api::get_settings().await.ok() });
    let recent = LocalResource::new(|| async move { api::list_recent_chats().await });
    let characters = LocalResource::new(|| async move { api::list_characters().await });

    view! {
        <div class="desk">
            {move || {
                let forbid_media = settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);
                let model = settings.get().flatten().map(|s| s.model_name).unwrap_or_default();
                let model_line = if model.trim().is_empty() {
                    view! {
                        <A href="/settings" attr:class="desk-model desk-model-missing">"No model set"</A>
                    }.into_any()
                } else {
                    view! {
                        <A href="/settings" attr:class="desk-model">{model}</A>
                    }.into_any()
                };

                let body = match (recent.get(), characters.get()) {
                    (None, _) | (_, None) => view! {
                        <p class="desk-status">"Loading…"</p>
                    }.into_any(),
                    (Some(Err(err)), _) => view! {
                        <p class="error">{err}</p>
                    }.into_any(),
                    (_, Some(Err(err))) => view! {
                        <p class="error">{err}</p>
                    }.into_any(),
                    (Some(Ok(chats)), Some(Ok(people))) => {
                        let chats_view = if chats.is_empty() {
                            ().into_any()
                        } else {
                            view! {
                                <section class="desk-section">
                                    <h1>"Continue"</h1>
                                    <div class="desk-list">
                                        {chats.into_iter().map(|chat| view! {
                                            <ChatRow chat=chat forbid_media=forbid_media />
                                        }).collect_view()}
                                    </div>
                                </section>
                            }.into_any()
                        };
                        let people_view = if people.is_empty() {
                            view! {
                                <section class="desk-section">
                                    <h1>"Characters"</h1>
                                    <p class="desk-status">"No characters yet."</p>
                                </section>
                            }.into_any()
                        } else {
                            let shown: Vec<Character> = people.into_iter().take(8).collect();
                            view! {
                                <section class="desk-section">
                                    <h1>"Characters"</h1>
                                    <div class="desk-list">
                                        {shown.into_iter().map(|character| view! {
                                            <CharacterRow character=character forbid_media=forbid_media />
                                        }).collect_view()}
                                    </div>
                                </section>
                            }.into_any()
                        };
                        view! {
                            {chats_view}
                            {people_view}
                            <div class="desk-actions">
                                <A href="/characters/new/edit" attr:class="desk-primary">"New character"</A>
                            </div>
                        }.into_any()
                    }
                };

                view! {
                    {model_line}
                    {body}
                }
            }}
        </div>
    }
}
