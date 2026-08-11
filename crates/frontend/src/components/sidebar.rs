use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

/// whether the mobile nav drawer is currently open. provided by `Sidebar`,
/// read by the hamburger button, the backdrop, and any navigation link
/// that should close the drawer before routing away (see
/// `character_list.rs`'s `CharacterListItem`). irrelevant above the 768px
/// breakpoint, where the drawer is always visible and this signal is
/// simply never toggled.
#[derive(Clone, Copy)]
pub struct SidebarDrawer(pub RwSignal<bool>);

#[component]
pub fn Sidebar(children: Children) -> impl IntoView {

    let drawer_open = RwSignal::new(false);
    provide_context(SidebarDrawer(drawer_open));

    let navigate = use_navigate();
    let go_to = {
        let nav = navigate.clone();
        move |path: &'static str| {
            drawer_open.set(false);
            nav(path, NavigateOptions::default());
        }
    };

    view! {
        <div class="app-shell">
            <div class="mobile-topbar">
                <button
                    class="hamburger"
                    on:click=move |_| drawer_open.update(|open| *open = !*open)
                >
                    "☰"
                </button>
                <span class="mobile-wordmark" style="cursor: pointer;" on:click={let go = go_to.clone(); move |_| go("/")}>"aetheria"</span>
            </div>
            <div
                class="sidebar-backdrop"
                class:open=move || drawer_open.get()
                on:click=move |_| drawer_open.set(false)
            ></div>
            <aside class="sidebar" class:open=move || drawer_open.get()>
                <div class="sidebar-wordmark" style="cursor: pointer;" on:click={let go = go_to.clone(); move |_| go("/")}>"aetheria."</div>
                <div class="sidebar-nav" style="display: flex; flex-direction: column; gap: 1.25rem; margin-top: 1rem;">
                    <div class="sidebar-nav-link" on:click={let go = go_to.clone(); move |_| go("/")}>"Desk"</div>
                    <div class="sidebar-nav-link" on:click={let go = go_to.clone(); move |_| go("/characters")}>"Library"</div>
                    <div class="sidebar-nav-link" on:click={let go = go_to.clone(); move |_| go("/lorebooks")}>"Lorebooks"</div>
                    <div class="sidebar-nav-link" on:click={let go = go_to.clone(); move |_| go("/presets")}>"Presets"</div>
                    <div class="sidebar-nav-link" on:click={let go = go_to.clone(); move |_| go("/settings")}>"System"</div>
                </div>
            </aside>
            <main class="main-content">{children()}</main>
        </div>
    }
}
