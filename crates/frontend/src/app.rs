use crate::components::sidebar::Sidebar;
use crate::pages::character_chats::CharacterChatsPage;
use crate::pages::character_editor::CharacterEditorPage;
use crate::pages::character_profile::CharacterProfilePage;
use crate::pages::characters::CharactersPage;
use crate::pages::chat::ChatPage;
use crate::pages::login::LoginPage;
use crate::pages::settings::SettingsPage;
use crate::pages::lorebooks::{LorebooksPage, LorebookEditorPage};
use crate::pages::presets::{PresetsPage, PresetEditorPage};
use crate::pages::themes::{ThemesPage, ThemeEditorPage};
use crate::pages::landing::LandingPage;
use crate::pages::terms::TermsPage;
use crate::pages::privacy::PrivacyPage;
use crate::theme::ThemeStore;
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{ParamSegment, StaticSegment};

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div style="display: flex; height: 100vh; width: 100vw; flex-direction: column; align-items: center; justify-content: center; background: #0b0b0e; color: #a89fc2; font-family: monospace;">
            <h1 style="font-size: 5rem; margin: 0; color: #ff4444;">"404"</h1>
            <p style="font-size: 1.2rem; margin-top: 1rem;">"The void stares back. No page found."</p>
            <a href="/" style="margin-top: 2rem; color: #ffffff; text-decoration: none; border-bottom: 1px solid #333339; padding-bottom: 0.2rem; transition: all 0.2s;">
                "Return to Aetheria"
            </a>
        </div>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let settings = LocalResource::new(|| async move { crate::api::get_settings().await });

    let active_theme = LocalResource::new(|| async move { crate::api::get_active_theme().await.unwrap_or_default() });
    let theme_tokens = RwSignal::new(crate::api::ThemeTokens::default());
    provide_context(ThemeStore(theme_tokens));

    Effect::new(move |_| {
        if let Some(tokens) = active_theme.get() {
            theme_tokens.set(tokens.clone());
        }
    });
    Effect::new(move |_| {
        crate::theme::apply_tokens_to_root(&theme_tokens.get());
    });

    view! {
        <Suspense fallback=|| view! { <div style="display: flex; height: 100vh; background: #0b0b0e;"></div> }>
            {move || match settings.get() {
                Some(Ok(_)) => view! {
                    <Router>
                        <crate::components::toast::ToastContainer />
                        <crate::components::mascot::MascotPeekCorner />
                        <Routes fallback=|| view! { <NotFoundPage /> }>
                            <Route
                                path=StaticSegment("characters")
                                view=|| view! { <Sidebar><CharactersPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <Sidebar><CharacterEditorPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"), StaticSegment("chats"))
                                view=|| view! { <Sidebar><CharacterChatsPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"))
                                view=|| view! { <Sidebar><CharacterProfilePage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("chat"), ParamSegment("id"))
                                view=|| view! { <Sidebar><ChatPage /></Sidebar> }
                            />
                            <Route
                                path=StaticSegment("lorebooks")
                                view=|| view! { <Sidebar><LorebooksPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("lorebooks"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <Sidebar><LorebookEditorPage /></Sidebar> }
                            />
                            <Route
                                path=StaticSegment("presets")
                                view=|| view! { <Sidebar><PresetsPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("presets"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <Sidebar><PresetEditorPage /></Sidebar> }
                            />
                            <Route
                                path=StaticSegment("themes")
                                view=|| view! { <Sidebar><ThemesPage /></Sidebar> }
                            />
                            <Route
                                path=(StaticSegment("themes"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <Sidebar><ThemeEditorPage /></Sidebar> }
                            />
                            <Route
                                path=StaticSegment("settings")
                                view=|| view! { <Sidebar><SettingsPage /></Sidebar> }
                            />
                            <Route
                                path=StaticSegment("terms")
                                view=|| view! { <TermsPage /> }
                            />
                            <Route
                                path=StaticSegment("privacy")
                                view=|| view! { <PrivacyPage /> }
                            />
                            <Route
                                path=StaticSegment("")
                                view=|| view! { <Sidebar><LandingPage /></Sidebar> }
                            />
                        </Routes>
                    </Router>
                }.into_any(),
                Some(Err(_)) => {
                    let path = web_sys::window()
                        .and_then(|w| w.location().pathname().ok())
                        .unwrap_or_default();
                    match path.as_str() {
                        "/terms" => view! { <TermsPage /> }.into_any(),
                        "/privacy" => view! { <PrivacyPage /> }.into_any(),
                        _ => view! { <LoginPage /> }.into_any(),
                    }
                }
                None => view! { <div style="display: flex; height: 100vh; background: #0b0b0e;"></div> }.into_any(),
            }}
        </Suspense>
    }
}
