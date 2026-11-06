use crate::api::{self};
use crate::components::character_list::{CharacterList, CharactersVersion};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn CharactersPage() -> impl IntoView {
    let characters_version = RwSignal::new(0);
    provide_context(CharactersVersion(characters_version));

    let (search, set_search) = signal(String::new());

    let (error, set_error) = signal(Option::<String>::None);
    let (importing, set_importing) = signal(false);

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
        <div class="character-library">
            <header class="collection-header">
                <h1 class="page-title">"library"</h1>
                <leptos_router::components::A href="/characters/new/edit" attr:class="desk-primary">"new character"</leptos_router::components::A>
            </header>
            <div class="collection-toolbar">
                <input type="search" aria-label="search characters" placeholder="search by name or tag"
                    prop:value=search on:input=move |ev| set_search.set(event_target_value(&ev)) />
                <label class="file-upload-label" for="import-upload">
                    {move || if importing.get() { "importing…" } else { "import png / json" }}
                </label>
                <input type="file" id="import-upload" class="file-upload-input" accept=".png,.json"
                    disabled=move || importing.get() on:change=on_import />
            </div>
            {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            <CharacterList filter_query=Signal::derive(move || search.get()) />
        </div>
    }
}
