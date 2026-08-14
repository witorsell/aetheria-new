use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[derive(Clone, Copy, PartialEq)]
pub enum MascotState {
    Idle,
    Thinking,
    Error,
    Empty,
    Lost,
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
            MascotState::Lost => "lost",
            MascotState::Peek => "peek",
            MascotState::PeekClicked => "peek-clicked",
        }
    }
}

/// Aeth, the mascot. Hidden entirely via the `.mascot-disabled .mascot { display: none; }`
/// rule in style.css when the active theme has `mascot_enabled: false`, so
/// this component doesn't need to read `ThemeStore` itself.
// bump this whenever any file in assets/mascot/ changes - cloudflare
// caches image responses at the edge for hours regardless of browser
// cache state, so the url itself has to change or a stale pop keeps
// serving the old art indefinitely
const MASCOT_ASSET_VERSION: u32 = 5;

#[component]
pub fn Aeth(#[prop(into)] state: Signal<MascotState>, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <img
            class=format!("mascot {}", class)
            src=move || format!("/assets/mascot/{}.png?v={}", state.get().asset_name(), MASCOT_ASSET_VERSION)
            alt="Aeth"
        />
    }
}

/// a low-frequency corner easter egg. hidden on `/chat/*` since that page
/// already pins its own contextual Aeth (idle/thinking/error) in the
/// corner via `mascot-chat-corner`.
#[component]
pub fn MascotPeekCorner() -> impl IntoView {
    let location = use_location();
    let on_chat_page = move || location.pathname.get().starts_with("/chat/");

    let (raised, set_raised) = signal(false);
    let (pose_clicked, set_pose_clicked) = signal(false);
    // bumped on every open/close so a stale close's delayed art-swap can
    // detect it's been superseded by a newer open and skip itself.
    let (close_gen, set_close_gen) = signal(0u32);

    let state = Signal::derive(move || if pose_clicked.get() { MascotState::PeekClicked } else { MascotState::Peek });

    let close = move || {
        set_raised.set(false);
        let my_gen = close_gen.get_untracked() + 1;
        set_close_gen.set(my_gen);
        set_timeout(
            move || {
                if close_gen.get_untracked() == my_gen {
                    set_pose_clicked.set(false);
                }
            },
            std::time::Duration::from_millis(220),
        );
    };

    let open = move || {
        set_close_gen.update(|g| *g += 1);
        set_pose_clicked.set(true);
        set_raised.set(true);
    };

    let on_toggle = move |_| {
        if raised.get_untracked() {
            close();
        } else {
            open();
        }
    };

    view! {
        {move || {
            if on_chat_page() {
                ().into_any()
            } else {
                view! {
                    {move || raised.get().then(|| view! {
                        <div class="mascot-corner-backdrop" on:click=move |_| close()></div>
                    })}
                    // one persistent element with a reactive class, not a branch swap,
                    // so the bottom/width CSS transition actually animates on close
                    // instead of snapping the box straight to its resting size.
                    <div class="mascot-corner" class:clicked=move || raised.get() on:click=on_toggle>
                        <Aeth state=state />
                    </div>
                }.into_any()
            }
        }}
    }
}
