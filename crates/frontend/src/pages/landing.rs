use crate::api::{self, CharacterSummary, RecentChat};
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
    let failed = RwSignal::new(false);
    view! {
        <div class="desk-avatar" aria-hidden="true">
            <span>{letter}</span>
            {url.filter(|_| !forbid_media).map(|url| view! {
                <img src=url alt="" loading="lazy" decoding="async" hidden=move || failed.get()
                    on:error=move |_| failed.set(true) />
            })}
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
fn CharacterRow(character: CharacterSummary, forbid_media: bool) -> impl IntoView {
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
    let characters = LocalResource::new(|| async move { api::list_character_summaries(Some(8)).await });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);

    view! {
        <div class="desk">
            <header class="collection-header">
                <h1 class="page-title">"desk"</h1>
                <A href="/characters/new/edit" attr:class="desk-primary">"new character"</A>
            </header>
            <Suspense fallback=|| ()>
                {move || settings.get().flatten().map(|s| view! {
                    <A href="/settings" attr:class="desk-model">
                        {if s.model_name.trim().is_empty() { "choose a model".to_string() } else { s.model_name }}
                    </A>
                })}
            </Suspense>
            <div class="desk-columns">
                <section class="desk-section">
                    <h2>"continue"</h2>
                    <Suspense fallback=|| view! { <crate::app::PageLoading /> }>
                        {move || match recent.get() {
                            None => ().into_any(),
                            Some(Err(error)) => view! { <p class="error" role="alert">{error}</p><button on:click=move |_| recent.refetch()>"try again"</button> }.into_any(),
                            Some(Ok(chats)) if chats.is_empty() => view! { <p class="desk-status">"your recent chats will appear here."</p> }.into_any(),
                            Some(Ok(chats)) => view! { <div class="desk-list">{chats.into_iter().map(|chat| view! { <ChatRow chat=chat forbid_media=forbid_media() /> }).collect_view()}</div> }.into_any(),
                        }}
                    </Suspense>
                </section>
                <section class="desk-section">
                    <header class="section-header"><h2>"characters"</h2><A href="/characters">"view all"</A></header>
                    <Suspense fallback=|| view! { <crate::app::PageLoading /> }>
                        {move || match characters.get() {
                            None => ().into_any(),
                            Some(Err(error)) => view! { <p class="error" role="alert">{error}</p><button on:click=move |_| characters.refetch()>"try again"</button> }.into_any(),
                            Some(Ok(people)) if people.is_empty() => view! { <p class="desk-status">"create or import your first character."</p> }.into_any(),
                            Some(Ok(people)) => view! { <div class="desk-list">{people.into_iter().map(|character| view! { <CharacterRow character=character forbid_media=forbid_media() /> }).collect_view()}</div> }.into_any(),
                        }}
                    </Suspense>
                </section>
            </div>
        </div>
    }
}
