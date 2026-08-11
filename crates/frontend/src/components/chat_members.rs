use crate::api::{self, Chat, Character, GroupWithMembers};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn ChatMembers(
    chat: Signal<Chat>,
    group: Signal<Option<GroupWithMembers>>,
    all_characters: Signal<Vec<Character>>,
    on_change: Callback<()>,
) -> impl IntoView {
    let (show_panel, set_show_panel) = signal(false);
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let member_ids = Signal::derive(move || {
        let g = group.get();
        let mut ids: Vec<String> = g.map(|g| g.members.iter().map(|m| m.character_id.clone()).collect()).unwrap_or_default();
        if ids.is_empty() {
            if let Some(cid) = chat.get().character_id {
                ids.push(cid);
            }
        }
        ids
    });

    let name_for = move |id: &str| -> String {
        all_characters.get().into_iter().find(|c| c.id == id).map(|c| c.name).unwrap_or_else(|| "unknown".to_string())
    };

    let add_character = move |character_id: String| {
        let chat_id = chat.get_untracked().id;
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::add_chat_member(&chat_id, &character_id).await {
                Ok(_) => on_change.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    let remove_character = move |character_id: String| {
        let chat_id = chat.get_untracked().id;
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::remove_chat_member(&chat_id, &character_id).await {
                Ok(_) => on_change.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    let move_member = move |character_id: String, direction: i32| {
        let g = group.get_untracked();
        let Some(g) = g else { return };
        let mut ids: Vec<String> = g.members.iter().map(|m| m.character_id.clone()).collect();
        let Some(idx) = ids.iter().position(|id| id == &character_id) else { return };
        let new_idx = idx as i32 + direction;
        if new_idx < 0 || new_idx as usize >= ids.len() {
            return;
        }
        ids.swap(idx, new_idx as usize);
        let disabled_by_id: std::collections::HashMap<String, bool> =
            g.members.iter().map(|m| (m.character_id.clone(), m.disabled)).collect();
        let ordered: Vec<(String, bool)> = ids.into_iter().map(|id| {
            let disabled = *disabled_by_id.get(&id).unwrap_or(&false);
            (id, disabled)
        }).collect();

        let group_id = g.group.id.clone();
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::set_group_members(&group_id, ordered).await {
                Ok(_) => on_change.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    let rename = move |new_name: String| {
        let Some(g) = group.get_untracked() else { return };
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::update_group(&g.group.id, &new_name, g.group.avatar_url.as_deref(), &g.group.activation_strategy).await {
                Ok(_) => on_change.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    let set_strategy = move |strategy: String| {
        let Some(g) = group.get_untracked() else { return };
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::update_group(&g.group.id, &g.group.name, g.group.avatar_url.as_deref(), &strategy).await {
                Ok(_) => on_change.run(()),
                Err(e) => set_error.set(Some(e)),
            }
            set_busy.set(false);
        });
    };

    view! {
        <button class="icon-btn" title="Members" on:click=move |_| set_show_panel.set(true)>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"></path>
                <circle cx="9" cy="7" r="4"></circle>
                <path d="M23 21v-2a4 4 0 0 0-3-3.87"></path>
                <path d="M16 3.13a4 4 0 0 1 0 7.75"></path>
            </svg>
        </button>

        {move || {
            if !show_panel.get() {
                return ().into_any();
            }
            let ids = member_ids.get();
            let is_group = group.get().is_some();
            view! {
                <div class="modal-backdrop" on:click=move |_| set_show_panel.set(false)>
                    <div class="modal-box" on:click=|ev| ev.stop_propagation()>
                        <div class="modal-header">
                            <strong>"Members"</strong>
                            <button class="icon-btn" title="Close" on:click=move |_| set_show_panel.set(false)>"x"</button>
                        </div>
                        <div class="modal-body" style="padding: 1rem;">
                            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}

                            {is_group.then(|| {
                                let g = group.get().unwrap();
                                view! {
                                    <div class="field">
                                        <label>"Group name"</label>
                                        <input
                                            type="text"
                                            prop:value=g.group.name.clone()
                                            on:change=move |ev| rename(event_target_value(&ev))
                                        />
                                    </div>
                                    <div class="field">
                                        <label>"Activation strategy"</label>
                                        <select on:change=move |ev| set_strategy(event_target_value(&ev))>
                                            <option value="list" selected=g.group.activation_strategy == "list">"List (everyone replies in order)"</option>
                                            <option value="natural" selected=g.group.activation_strategy == "natural">"Natural (mentions + talkativeness)"</option>
                                        </select>
                                    </div>
                                }.into_any()
                            }).unwrap_or_else(|| ().into_any())}

                            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin: 1rem 0;">
                                {ids.iter().enumerate().map(|(idx, id)| {
                                    let id = id.clone();
                                    let id_remove = id.clone();
                                    let id_up = id.clone();
                                    let id_down = id.clone();
                                    let can_remove = ids.len() > 1;
                                    let can_move_up = idx > 0;
                                    let can_move_down = idx + 1 < ids.len();
                                    view! {
                                        <div style="display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem; border: 1px solid var(--color-border);">
                                            <span style="flex: 1;">{name_for(&id)}</span>
                                            <button class="ghost small" disabled=!can_move_up || busy.get()
                                                on:click=move |_| move_member(id_up.clone(), -1)>"^"</button>
                                            <button class="ghost small" disabled=!can_move_down || busy.get()
                                                on:click=move |_| move_member(id_down.clone(), 1)>"v"</button>
                                            <button class="ghost small" disabled=!can_remove || busy.get()
                                                on:click=move |_| remove_character(id_remove.clone())>"remove"</button>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>

                            <div class="field">
                                <label>"Add a character"</label>
                                <select
                                    disabled=busy.get()
                                    on:change=move |ev| {
                                        let id = event_target_value(&ev);
                                        if !id.is_empty() {
                                            add_character(id);
                                        }
                                    }
                                >
                                    <option value="">"Pick a character..."</option>
                                    {all_characters.get().into_iter()
                                        .filter(|c| !member_ids.get_untracked().contains(&c.id))
                                        .map(|c| view! { <option value=c.id.clone()>{c.name.clone()}</option> })
                                        .collect::<Vec<_>>()}
                                </select>
                            </div>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
