use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug)]
pub struct ToastMessage {
    pub id: u64,
    pub text: String,
    pub kind: ToastKind,
}

/// global toast notification store for pushing non-blocking ui banners
#[derive(Clone, Copy)]
pub struct ToastStore {
    pub messages: RwSignal<Vec<ToastMessage>>,
}

impl ToastStore {
    pub fn push(&self, text: impl Into<String>, kind: ToastKind) {
        let msg = ToastMessage {
            id: (js_sys::Math::random() * 1_000_000_000.0) as u64,
            text: text.into(),
            kind,
        };
        self.messages.update(|list| list.push(msg));
    }
}

#[component]
pub fn ToastContainer() -> impl IntoView {
    let toast_store = use_context::<ToastStore>().unwrap_or_else(|| {
        let store = ToastStore { messages: RwSignal::new(Vec::new()) };
        provide_context(store);
        store
    });

    view! {
        <div style="position: fixed; bottom: 1.5rem; right: 1.5rem; z-index: 9999; display: flex; flex-direction: column; gap: 0.5rem; pointer-events: none;">
            <For
                each=move || toast_store.messages.get()
                key=|m| m.id
                children=move |msg| {
                    let bg = match msg.kind {
                        ToastKind::Error => "rgba(220, 38, 38, 0.95)",
                        ToastKind::Success => "rgba(16, 185, 129, 0.95)",
                        ToastKind::Info => "rgba(59, 130, 246, 0.95)",
                    };
                    view! {
                        <div style=format!("background: {bg}; color: #ffffff; padding: 0.75rem 1.25rem; border-radius: 8px; font-size: 0.9rem; box-shadow: 0 4px 12px rgba(0,0,0,0.3); pointer-events: auto; font-family: sans-serif;")>
                            {msg.text}
                        </div>
                    }
                }
            />
        </div>
    }
}
