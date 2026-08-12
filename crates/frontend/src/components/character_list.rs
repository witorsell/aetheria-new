use crate::api::{self, Character};
use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

/// shared trigger that lets any descendant (e.g. the Characters page's
/// create form) tell `CharacterList` to refetch, without both sides needing
/// to share the same `LocalResource`. provided by `Sidebar` via
/// `provide_context`, bumped by whoever creates a character.
#[derive(Clone, Copy)]
pub struct CharactersVersion(pub RwSignal<u32>);

/// the character list with click-to-open-or-create-chat behavior. used by
/// both the sidebar (compact, always visible) and the Characters page
/// (fuller list alongside the create form).
#[component]
pub fn CharacterList() -> impl IntoView {
    let version = use_context::<CharactersVersion>()
        .map(|v| v.0)
        .unwrap_or_else(|| RwSignal::new(0));

    let navigate = use_navigate();
    let characters = LocalResource::new(move || {
        version.get();
        let navigate = navigate.clone();
        async move { 
            match api::list_characters().await {
                Ok(c) => c,
                Err(_) => {
                    navigate("/login", Default::default());
                    vec![]
                }
            }
        }
    });

    let settings = leptos::prelude::LocalResource::new(|| async move { crate::api::get_settings().await.ok() });
    let forbid_media = Signal::derive(move || settings.get().flatten().map(|s| s.forbid_external_media).unwrap_or(false));

    view! {
        <div class="sidebar-characters">
            {move || {
                match characters.get() {
                    // still loading: `LocalResource::get()` is `None` until the
                    // first fetch resolves, distinct from "loaded and empty".
                    // rendering nothing here avoids a flash of the empty-state
                    // mascot on every page load, even for accounts with characters.
                    None => ().into_any(),
                    Some(list) if list.is_empty() => view! {
                        <div class="mascot-empty-state">
                            <crate::components::mascot::Aeth state=Signal::derive(|| crate::components::mascot::MascotState::Empty) class="mascot-empty" />
                            <p style="color: var(--color-text-muted); font-size: 0.875rem;">"No one's here yet."</p>
                        </div>
                    }.into_any(),
                    Some(list) => list.into_iter()
                        .map(|character: Character| view! { <CharacterListItem character=character forbid_media=forbid_media /> })
                        .collect_view()
                        .into_any(),
                }
            }}
        </div>
    }
}

#[component]
fn CharacterListItem(character: Character, #[prop(into)] forbid_media: Signal<bool>) -> impl IntoView {
    let navigate = use_navigate();
    let character_id = character.id.clone();
    let initial = character.name.chars().next().unwrap_or('?').to_uppercase().to_string();

    let nav_for_click = navigate.clone();
    let on_click = move |_| {
        let navigate = nav_for_click.clone();
        let character_id = character_id.clone();
        if let Some(drawer) = use_context::<crate::components::sidebar::SidebarDrawer>() {
            drawer.0.set(false);
        }
        navigate(&format!("/characters/{}", character_id), NavigateOptions::default());
    };

    view! {
        <div class="character-list-item" on:click=on_click>
            <div class="character-avatar">
                {move || {
                    if let Some(url) = character.avatar_url.clone().filter(|_| !forbid_media.get()) {
                        view! { <img src=url alt="avatar" /> }.into_any()
                    } else {
                        view! { {initial.clone()} }.into_any()
                    }
                }}
            </div>
            <span>{character.name}</span>
            <button class="ghost small edit-btn" on:click={
                let navigate = navigate.clone();
                let id = character.id.clone();
                move |ev| {
                    ev.stop_propagation();
                    navigate(&format!("/characters/{id}/edit"), NavigateOptions::default());
                }
            }>"✎"</button>

        </div>
    }
}
