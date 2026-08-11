use crate::api::{self, Character, Chat};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;
use std::sync::Arc;

type NavFn = Arc<dyn Fn(&str, NavigateOptions) + Send + Sync + 'static>;

fn format_relative(ts: i64) -> String {
    let now = js_sys::Date::now() as i64 / 1000;
    let secs = (now - ts).max(0);
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{}m ago", mins);
    }
    let hrs = mins / 60;
    if hrs < 24 {
        return format!("{}h ago", hrs);
    }
    let days = hrs / 24;
    if days < 30 {
        return format!("{}d ago", days);
    }
    format!("{}mo ago", days / 30)
}

#[component]
pub fn CharacterChatsPage() -> impl IntoView {
    let settings = leptos::prelude::LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);
    let params = use_params_map();
    let raw_nav = use_navigate();
    let nav: NavFn = Arc::new(move |p: &str, o| raw_nav(p, o));

    let char_id = move || params.read().get("id").unwrap_or_default();

    let character: LocalResource<Option<Character>> = LocalResource::new(move || {
        let id = char_id();
        async move { api::get_character(&id).await.ok() }
    });

    let chats: LocalResource<Vec<Chat>> = LocalResource::new(move || {
        let id = char_id();
        async move { api::list_chats(&id).await.unwrap_or_default() }
    });

    view! {
        <div class="chats-page">
            <Suspense fallback=|| view! { <div class="chats-page-loading">"Loading..."</div> }>
                {move || {
                    let nav = nav.clone();
                    let char_id = char_id();
                    let char = character.get().flatten();
                    let chat_list = chats.get().unwrap_or_default();

                    Some(view! {
                        <ChatsPageInner
                            char=char
                            chats=chat_list
                            char_id=char_id
                            nav=nav
                        />
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn ChatsPageInner(
    char: Option<Character>,
    chats: Vec<Chat>,
    char_id: String,
    nav: NavFn,
) -> impl IntoView {
    let settings = leptos::prelude::LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false);
    let nav_back = nav.clone();
    let nav_new = nav.clone();
    let nav_items = nav.clone();
    let cid_new = char_id.clone();
    let char_name = char.as_ref().map(|c| c.name.clone()).unwrap_or_default();
    let avatar_url = char.as_ref().and_then(|c| c.avatar_url.clone());
    let initial = char_name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let back_id = char_id.clone();

    let (creating, set_creating) = signal(false);

    view! {
        <div>
            <nav class="chats-nav">
                <button class="ghost" on:click=move |_| {
                    nav_back(&format!("/characters/{}", back_id), NavigateOptions::default());
                }>"←"</button>
                <div class="chats-nav-char">
                    {move || {
                    if let Some(url) = avatar_url.clone().filter(|_| !forbid_media()) {
                        view! { <img src=url class="chats-nav-avatar" /> }.into_any()
                    } else {
                        view! { <div class="chats-nav-avatar chats-nav-avatar-fallback">{initial.clone()}</div> }.into_any()
                    }
                }}
                    <span class="chats-nav-name">{char_name.clone()}</span>
                </div>
            </nav>

            <div class="chats-header">
                <div>
                    <h1 class="chats-title">"Conversations"</h1>
                    <p class="chats-subtitle">{
                        let n = chats.len();
                        if n == 0 { "No conversations yet.".to_string() }
                        else if n == 1 { "1 conversation".to_string() }
                        else { format!("{} conversations", n) }
                    }</p>
                </div>
                <button class="primary" disabled=move || creating.get() on:click=move |_| {
                    set_creating.set(true);
                    let nav = nav_new.clone();
                    let id = cid_new.clone();
                    spawn_local(async move {
                        match api::create_chat(&id, "Chat").await {
                            Ok(chat) => nav(&format!("/chat/{}", chat.id), NavigateOptions::default()),
                            Err(_) => set_creating.set(false),
                        }
                    });
                }>
                    {move || if creating.get() { "Starting…" } else { "New Chat" }}
                </button>
            </div>

            {if chats.is_empty() {
                view! {
                    <div class="chats-empty">
                        <div class="chats-empty-icon">"✦"</div>
                        <p>"No conversations with " <i>{char_name}</i> " yet."</p>
                        <p class="chats-empty-hint">"Hit New Chat to begin."</p>
                    </div>
                }.into_any()
            } else {
                chats.into_iter().map(|chat| {
                    let nav = nav_items.clone();
                    let chat_id = chat.id.clone();
                    let title = if chat.title.is_empty() { "Untitled".to_string() } else { chat.title.clone() };
                    let rel_time = "Recently".to_string();
                    let is_group = chat.group_id.is_some();
                    view! {
                        <div class="chat-row" on:click=move |_| {
                            nav(&format!("/chat/{}", chat_id), NavigateOptions::default());
                        }>
                            <div class="chat-row-main">
                                <span class="chat-row-title">
                                    {is_group.then(|| view! { <span class="chat-row-group-badge" title="Group chat">"👥"</span> })}
                                    {title}
                                </span>
                                <span class="chat-row-time">{rel_time}</span>
                            </div>
                            <span class="chat-row-arrow">"›"</span>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </div>
    }
}
