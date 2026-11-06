use crate::api::{self, CharacterSummary};
use leptos::prelude::*;
use leptos_router::components::A;
use std::collections::HashMap;

/// shared trigger that lets any descendant (e.g. the Characters page's
/// create form) tell `CharacterList` to refetch, without both sides needing
/// to share the same `LocalResource`. provided by `Sidebar` via
/// `provide_context`, bumped by whoever creates a character.
#[derive(Clone, Copy)]
pub struct CharactersVersion(pub RwSignal<u32>);

/// the character list with click-to-open-or-create-chat behavior. used by
/// both the sidebar (compact, always visible) and the Characters page
/// (fuller list alongside the create form).
/// when set, enables tag badges on each character and filters the list down to
/// whatever's typed in the signal, matched against character names and tag
/// names (case-insensitive substring, empty = show everything). left unset by
/// callers (the sidebar's compact list) to skip the extra tag-data fetch
/// entirely and keep that view unchanged.
#[component]
pub fn CharacterList(#[prop(optional)] filter_query: Option<Signal<String>>) -> impl IntoView {
    let version = use_context::<CharactersVersion>()
        .map(|v| v.0)
        .unwrap_or_else(|| RwSignal::new(0));

    let characters = LocalResource::new(move || {
        version.get();
        async move { api::list_character_summaries(None).await }
    });

    let show_tags = filter_query.is_some();
    let character_tags = LocalResource::new(move || {
        version.get();
        async move {
            if show_tags {
                api::list_all_character_tags().await.unwrap_or_default()
            } else {
                HashMap::new()
            }
        }
    });
    let all_tags = LocalResource::new(move || {
        version.get();
        async move {
            if show_tags {
                api::list_tags().await.unwrap_or_default()
            } else {
                Vec::new()
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
                    None => view! { <crate::app::PageLoading /> }.into_any(),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <div class="mascot-empty-state">
                            <crate::components::mascot::Aeth state=Signal::derive(|| crate::components::mascot::MascotState::Empty) class="mascot-empty" />
                            <p style="color: var(--color-text-muted); font-size: 0.875rem;">"No one's here yet."</p>
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <div role="alert"><p class="error">{error}</p>
                            <button on:click=move |_| characters.refetch()>"try again"</button>
                        </div>
                    }.into_any(),
                    Some(Ok(list)) => {
                        let tags_by_character = character_tags.get().unwrap_or_default();
                        let tags_by_id: HashMap<String, api::Tag> = all_tags.get().unwrap_or_default()
                            .into_iter().map(|t| (t.id.clone(), t)).collect();
                        let query = filter_query.map(|f| f.get()).unwrap_or_default();
                        // each whitespace-separated term must match somewhere - name or a tag name -
                        // so multiple terms narrow the results down instead of widening them.
                        let terms: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();

                        let filtered: Vec<CharacterSummary> = list.into_iter().filter(|c| {
                            if terms.is_empty() {
                                return true;
                            }
                            let name_lower = c.name.to_lowercase();
                            let tag_names: Vec<String> = tags_by_character.get(&c.id)
                                .map(|ids| ids.iter().filter_map(|id| tags_by_id.get(id)).map(|t| t.name.to_lowercase()).collect())
                                .unwrap_or_default();
                            terms.iter().all(|term| {
                                name_lower.contains(term.as_str()) || tag_names.iter().any(|t| t.contains(term.as_str()))
                            })
                        }).collect();

                        if filtered.is_empty() {
                            view! {
                                <p style="color: var(--color-text-muted); font-size: 0.875rem; padding: 1rem;">"No characters match that search."</p>
                            }.into_any()
                        } else {
                            filtered.into_iter()
                                .map(|character: CharacterSummary| {
                                    let tags: Vec<api::Tag> = tags_by_character.get(&character.id)
                                        .map(|ids| ids.iter().filter_map(|id| tags_by_id.get(id).cloned()).collect())
                                        .unwrap_or_default();
                                    view! { <CharacterListItem character=character forbid_media=forbid_media tags=tags /> }
                                })
                                .collect_view()
                                .into_any()
                        }
                    }
                }
            }}
        </div>
    }
}

#[component]
fn CharacterListItem(character: CharacterSummary, #[prop(into)] forbid_media: Signal<bool>, #[prop(optional)] tags: Vec<api::Tag>) -> impl IntoView {
    let href = format!("/characters/{}", character.id);
    let edit_href = format!("/characters/{}/edit", character.id);
    let initial = character.name.chars().next().unwrap_or('?').to_uppercase().to_string();
    view! {
        <div class="character-list-item">
            <A href=href attr:class="character-profile-link">
            <div class="character-avatar">
                {move || {
                    if let Some(url) = character.avatar_url.clone().filter(|_| !forbid_media.get()) {
                        view! { <img src=url alt="" loading="lazy" decoding="async" /> }.into_any()
                    } else {
                        view! { {initial.clone()} }.into_any()
                    }
                }}
            </div>
            <div style="display: flex; flex-direction: column; gap: 0.2rem; min-width: 0;">
                <span>{character.name}</span>
                {if tags.is_empty() {
                    view! {}.into_any()
                } else {
                    view! {
                        <div style="display: flex; gap: 0.3rem; flex-wrap: wrap;">
                            {tags.into_iter().map(|t| view! {
                                <span
                                    title=t.name.clone()
                                    style=format!("width: 0.5rem; height: 0.5rem; border-radius: 50%; background: {}; flex-shrink: 0;", t.color)
                                ></span>
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
            </A>
            <A href=edit_href attr:class="edit-btn" attr:aria-label="edit character">"edit"</A>
        </div>
    }
}
