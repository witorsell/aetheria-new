use leptos::prelude::*;

/// pulsing skeleton loader for character cards and library items
#[component]
pub fn SkeletonCard() -> impl IntoView {
    view! {
        <div style="background: rgba(255, 255, 255, 0.03); border: 1px solid rgba(255, 255, 255, 0.06); border-radius: 12px; padding: 1rem; display: flex; gap: 1rem; align-items: center; animation: pulse 1.5s infinite ease-in-out;">
            <div style="width: 48px; height: 48px; border-radius: 50%; background: rgba(255, 255, 255, 0.08);"></div>
            <div style="flex: 1; display: flex; flex-direction: column; gap: 0.5rem;">
                <div style="width: 40%; height: 16px; border-radius: 4px; background: rgba(255, 255, 255, 0.08);"></div>
                <div style="width: 75%; height: 12px; border-radius: 4px; background: rgba(255, 255, 255, 0.05);"></div>
            </div>
        </div>
    }
}

/// pulsing skeleton placeholder for chat messages while fetching history
#[component]
pub fn SkeletonChatMessage() -> impl IntoView {
    view! {
        <div style="display: flex; gap: 1rem; padding: 1rem; border-radius: 8px; background: rgba(255, 255, 255, 0.02); margin-bottom: 1rem; animation: pulse 1.5s infinite ease-in-out;">
            <div style="width: 36px; height: 36px; border-radius: 50%; background: rgba(255, 255, 255, 0.08);"></div>
            <div style="flex: 1; display: flex; flex-direction: column; gap: 0.5rem;">
                <div style="width: 25%; height: 14px; border-radius: 4px; background: rgba(255, 255, 255, 0.08);"></div>
                <div style="width: 90%; height: 12px; border-radius: 4px; background: rgba(255, 255, 255, 0.05);"></div>
                <div style="width: 60%; height: 12px; border-radius: 4px; background: rgba(255, 255, 255, 0.05);"></div>
            </div>
        </div>
    }
}
