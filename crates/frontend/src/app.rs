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
use leptos_router::components::{Outlet, ParentRoute, Route, Router, Routes};
use leptos_router::{ParamSegment, StaticSegment};

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div style="display: flex; height: 100dvh; width: 100vw; flex-direction: column; align-items: center; justify-content: center; background: var(--color-bg); color: var(--color-text-muted); font-family: var(--font-body); text-align: center; padding: 2rem;">
            <div style="width: 170px;">
                <crate::components::mascot::Aeth state=Signal::derive(|| crate::components::mascot::MascotState::Lost) class="mascot-empty" />
            </div>
            <p style="font-family: monospace; font-size: 0.75rem; letter-spacing: 0.15em; text-transform: uppercase; color: var(--color-text-muted); margin: 0.5rem 0 0;">"Error 404"</p>
            <h1 style="font-family: var(--font-heading); font-weight: 300; font-size: 1.8rem; margin: 0.4rem 0 0; letter-spacing: -0.01em; color: var(--color-text-heading);">"Even Aeth couldn't find this one."</h1>
            <p style="font-size: 1rem; margin-top: 0.75rem; max-width: 32ch;">"The page you're looking for doesn't exist, or moved."</p>
            <a href="/" style="margin-top: 2rem; color: var(--color-text); text-decoration: none; border-bottom: 1px dotted var(--color-border); padding-bottom: 0.2rem; font-family: monospace; font-size: 0.85rem; letter-spacing: 0.05em; text-transform: uppercase;">
                "Return to Aetheria"
            </a>
        </div>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let session = LocalResource::new(|| async move { crate::api::fetch_me().await });

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
            {move || match session.get() {
                Some(Ok(_)) => view! {
                    <Router>
                        <crate::components::toast::ToastContainer />
                        <crate::components::mascot::MascotPeekCorner />
                        <Routes fallback=|| view! { <NotFoundPage /> }>
                            <ParentRoute path=StaticSegment("") view=|| view! { <Sidebar><Suspense fallback=|| view! { <crate::app::PageLoading /> }><Outlet /></Suspense></Sidebar> }>
                            <Route
                                path=StaticSegment("characters")
                                view=|| view! { <CharactersPage /> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <CharacterEditorPage /> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"), StaticSegment("chats"))
                                view=|| view! { <CharacterChatsPage /> }
                            />
                            <Route
                                path=(StaticSegment("characters"), ParamSegment("id"))
                                view=|| view! { <CharacterProfilePage /> }
                            />
                            <Route
                                path=(StaticSegment("chat"), ParamSegment("id"))
                                view=|| view! { <ChatPage /> }
                            />
                            <Route
                                path=StaticSegment("lorebooks")
                                view=|| view! { <LorebooksPage /> }
                            />
                            <Route
                                path=(StaticSegment("lorebooks"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <LorebookEditorPage /> }
                            />
                            <Route
                                path=StaticSegment("presets")
                                view=|| view! { <PresetsPage /> }
                            />
                            <Route
                                path=(StaticSegment("presets"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <PresetEditorPage /> }
                            />
                            <Route
                                path=StaticSegment("themes")
                                view=|| view! { <ThemesPage /> }
                            />
                            <Route
                                path=(StaticSegment("themes"), ParamSegment("id"), StaticSegment("edit"))
                                view=|| view! { <ThemeEditorPage /> }
                            />
                            <Route
                                path=StaticSegment("settings")
                                view=|| view! { <SettingsPage /> }
                            />
                            <Route path=StaticSegment("") view=|| view! { <LandingPage /> } />
                            <Route path=StaticSegment("login") view=|| view! { <LandingPage /> } />
                            </ParentRoute>
                            <Route
                                path=StaticSegment("terms")
                                view=|| view! { <TermsPage /> }
                            />
                            <Route
                                path=StaticSegment("privacy")
                                view=|| view! { <PrivacyPage /> }
                            />
                        </Routes>
                    </Router>
                }.into_any(),
                Some(Err(error)) if error != crate::api::SESSION_EXPIRED_ERROR => view! {
                    <div class="connection-error" role="alert">
                        <h1>"couldn't connect"</h1><p>{error}</p>
                        <button on:click=move |_| session.refetch()>"try again"</button>
                    </div>
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
                None => view! { <PageLoading /> }.into_any(),
            }}
    }
}

#[component]
pub fn PageLoading() -> impl IntoView {
    view! {
        <div class="page-loading" role="status" aria-live="polite">
            <p>"loading…"</p>
            <div class="loading-line"></div><div class="loading-line"></div><div class="loading-line"></div>
        </div>
    }
}
