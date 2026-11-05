use leptos::prelude::*;
use leptos_router::components::A;

/// whether the mobile nav drawer is currently open. provided by `Sidebar`,
/// read by the hamburger button, the backdrop, and any navigation link
/// that should close the drawer before routing away (see
/// `character_list.rs`'s `CharacterListItem`). irrelevant above the 768px
/// breakpoint, where the drawer is always visible and this signal is
/// simply never toggled.
#[derive(Clone, Copy)]
pub struct SidebarDrawer(pub RwSignal<bool>);

#[component]
fn NavLink(
    href: &'static str,
    #[prop(optional)] exact: bool,
    children: Children,
) -> impl IntoView {
    let drawer = use_context::<SidebarDrawer>();
    let close = move |_| {
        if let Some(SidebarDrawer(open)) = drawer {
            open.set(false);
        }
    };
    view! {
        <A href=href exact=exact attr:class="sidebar-nav-link" on:click=close>
            {children()}
        </A>
    }
}

#[component]
pub fn Sidebar(children: Children) -> impl IntoView {
    let drawer_open = RwSignal::new(false);
    provide_context(SidebarDrawer(drawer_open));

    let close_drawer = move |_| drawer_open.set(false);

    view! {
        <div class="app-shell">
            <div class="mobile-topbar">
                <button
                    class="hamburger"
                    aria-label="Menu"
                    on:click=move |_| drawer_open.update(|open| *open = !*open)
                >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M4 7h16"></path>
                        <path d="M4 12h16"></path>
                        <path d="M4 17h16"></path>
                    </svg>
                </button>
                <A href="/" attr:class="mobile-wordmark" on:click=close_drawer>"aetheria"</A>
            </div>
            <div
                class="sidebar-backdrop"
                class:open=move || drawer_open.get()
                on:click=move |_| drawer_open.set(false)
            ></div>
            <aside class="sidebar" class:open=move || drawer_open.get()>
                <A href="/" attr:class="sidebar-wordmark" on:click=move |_| drawer_open.set(false)>"aetheria"</A>
                <nav class="sidebar-nav">
                    <NavLink href="/" exact=true>"Desk"</NavLink>
                    <NavLink href="/characters">"Library"</NavLink>
                    <NavLink href="/lorebooks">"Lorebooks"</NavLink>
                    <NavLink href="/presets">"Presets"</NavLink>
                    <NavLink href="/themes">"Themes"</NavLink>
                    <NavLink href="/settings">"Settings"</NavLink>
                </nav>
                <form class="sidebar-foot" method="POST" action="/api/logout">
                    <button type="submit" class="sidebar-logout">"Log out"</button>
                </form>
            </aside>
            <main class="main-content">{children()}</main>
        </div>
    }
}
