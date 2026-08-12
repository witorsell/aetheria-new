use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum MascotState {
    Idle,
    Thinking,
    Error,
    Empty,
    Peek,
    PeekClicked,
}

impl MascotState {
    fn asset_name(&self) -> &'static str {
        match self {
            MascotState::Idle => "idle",
            MascotState::Thinking => "thinking",
            MascotState::Error => "error",
            MascotState::Empty => "empty",
            MascotState::Peek => "peek",
            MascotState::PeekClicked => "peek-clicked",
        }
    }
}

/// Aeth, the mascot. Hidden entirely via the `.mascot-disabled .mascot { display: none; }`
/// rule in style.css when the active theme has `mascot_enabled: false`, so
/// this component doesn't need to read `ThemeStore` itself.
#[component]
pub fn Aeth(#[prop(into)] state: Signal<MascotState>, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <img
            class=format!("mascot {}", class)
            src=move || format!("/assets/mascot/{}.png", state.get().asset_name())
            alt="Aeth"
        />
    }
}

/// a low-frequency corner easter egg: Aeth peeks up from the bottom edge,
/// and clicking swaps to the startled `peek-clicked` pose for a moment
/// before going back to hiding.
#[component]
pub fn MascotPeekCorner() -> impl IntoView {
    let (visible, set_visible) = signal(false);
    let (clicked, set_clicked) = signal(false);

    let state = Signal::derive(move || if clicked.get() { MascotState::PeekClicked } else { MascotState::Peek });

    let on_click = move |_| {
        set_clicked.set(true);
        set_timeout(move || {
            set_clicked.set(false);
            set_visible.set(false);
        }, std::time::Duration::from_millis(1200));
    };

    view! {
        <div class="mascot-corner" class:visible=move || visible.get() on:click=on_click>
            <Aeth state=state />
        </div>
    }
}
