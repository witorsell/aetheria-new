use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

const SPEED_PRESETS: [(u32, &str); 4] = [(0, "Disabled"), (20, "Slow"), (40, "Normal"), (80, "Fast")];

fn speed_to_preset_index(speed: u32) -> usize {
    SPEED_PRESETS
        .iter()
        .enumerate()
        .min_by_key(|(_, (value, _))| (*value as i64 - speed as i64).abs())
        .map(|(index, _)| index)
        .unwrap_or(2)
}

#[component]
pub fn LandingPage() -> impl IntoView {
    let navigate = use_navigate();
    let navigate_store = StoredValue::new(navigate.clone());
    let (hovered, set_hovered) = signal(false);

    let settings = LocalResource::new(|| async move { crate::api::get_settings().await });
    let (api_base_url, set_api_base_url) = signal(String::new());
    let (model_name, set_model_name) = signal(String::new());
    let (context_limit, set_context_limit) = signal(8192i64);
    let (speed_index, _) = signal(speed_to_preset_index(crate::api::get_text_speed()));

    Effect::new(move |_| {
        if let Some(Ok(view)) = settings.get() {
            set_api_base_url.set(view.api_base_url);
            set_model_name.set(view.model_name);
            set_context_limit.set(view.context_limit);
        }
    });

    view! {
        <div class="landing-page landing-layout">
            <div style="position: absolute; top: -20%; left: -10%; width: 50vw; height: 50vw; background: radial-gradient(circle, rgba(139, 92, 246, 0.03) 0%, rgba(10, 10, 12, 0) 70%); border-radius: 50%; pointer-events: none;"></div>
            
            <div class="landing-left">
                <div>
                    <h1 style="font-family: var(--font-heading); font-size: clamp(4rem, 10vw, 10rem); font-weight: 300; letter-spacing: -0.04em; color: #ffffff; margin: 0; line-height: 0.85; margin-bottom: 2rem;">
                        "Aetheria."
                    </h1>
                    <form method="POST" action="/api/logout">
                        <button type="submit" class="btn" style="background: transparent; border: 1px solid var(--color-border); color: var(--color-text-muted);">"Logout"</button>
                    </form>
                    <div style="margin-top: 4rem; max-width: 500px; display: grid; gap: 2rem;">
                        <p style="font-family: var(--font-body); font-size: 1.1rem; color: #8a8a93; line-height: 1.7; font-weight: 300;">
                            "The ink hasn't dried. The stage is empty. This is an archive of synthetic souls, waiting to be summoned from the margins."
                        </p>
                        
                        <div style="display: flex; gap: 2rem; align-items: center; padding-top: 2rem; border-top: 1px solid var(--color-border);">
                            <button 
                                style=move || format!("background: transparent; border: none; color: {}; font-family: var(--font-heading); font-size: 1.8rem; font-style: italic; padding: 0 0 0.2rem 0; cursor: pointer; border-bottom: 1px solid {}; transition: all 0.3s ease;", if hovered.get() { "#a89fc2" } else { "#ffffff" }, if hovered.get() { "rgba(168, 159, 194, 0.3)" } else { "rgba(255, 255, 255, 0.3)" })
                                on:mouseover=move |_| set_hovered.set(true)
                                on:mouseout=move |_| set_hovered.set(false)
                                on:click={
                                    let nav = navigate_store;
                                    move |_| nav.get_value()("/characters/new/edit", NavigateOptions::default())
                                }
                            >
                                "Begin drafting"
                            </button>
                        </div>
                    </div>
                </div>
                
                <div style="display: flex; gap: 2rem; flex-wrap: wrap; margin-top: 4rem; color: #4a4a53; font-family: monospace; font-size: 0.8rem; letter-spacing: 0.05em; text-transform: uppercase;">
                    <div>"SYS_VER_2.0.4"</div>
                    <div>"LOCAL_RUNTIME"</div>
                    <div>"PROTOCOL_STRICT"</div>
                    <a href="/terms" style="color: #4a4a53; text-decoration: none;">"TERMS"</a>
                    <a href="/privacy" style="color: #4a4a53; text-decoration: none;">"PRIVACY"</a>
                </div>
            </div>
            
            <div class="landing-right">
                <div style="display: flex; flex-direction: column; gap: 1.5rem;">
                    <div style="color: #ffffff; font-family: var(--font-heading); font-size: 2rem; font-style: italic; letter-spacing: -0.02em;">"System parameters."</div>
                    <p style="color: #6a6a73; font-size: 1.05rem; line-height: 1.6; max-width: 400px; font-weight: 300;">
                        "Configure the neural engine before stepping into the void."
                    </p>
                </div>
                
                <div style="display: flex; flex-direction: column; gap: 1.5rem; max-width: 400px; height: 100%; overflow-y: auto; padding-right: 1rem;" class="settings-display">
                    <div class="field" style="margin-bottom: 0;">
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem;">"Provider"</div>
                        <div style="color: #fff; font-family: monospace; font-size: 0.9rem; word-break: break-all;">{move || if api_base_url.get().is_empty() { "Not configured".to_string() } else { api_base_url.get() }}</div>
                    </div>
                    
                    <div class="field" style="margin-bottom: 0;">
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem;">"Model"</div>
                        <div style="color: #fff; font-family: monospace; font-size: 0.9rem;">{move || if model_name.get().is_empty() { "Not configured".to_string() } else { model_name.get() }}</div>
                    </div>

                    <div class="field" style="margin-bottom: 0;">
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem;">"Context Limit"</div>
                        <div style="color: #fff; font-family: monospace; font-size: 0.9rem;">{move || format!("{} tokens", context_limit.get())}</div>
                    </div>
                    
                    <div class="field" style="margin-bottom: 0;">
                        <div style="color: var(--color-text-muted); font-family: monospace; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem;">"Streaming Speed"</div>
                        <div style="color: #fff; font-family: monospace; font-size: 0.9rem;">{move || SPEED_PRESETS[speed_index.get()].1}</div>
                    </div>

                    <div style="margin-top: 2rem;">
                        <button 
                            style="background: transparent; border: none; color: #a89fc2; font-family: monospace; font-size: 0.9rem; padding: 0; cursor: pointer; border-bottom: 1px dotted rgba(168, 159, 194, 0.5); transition: all 0.3s ease;"
                            on:click={
                                let nav = navigate_store;
                                move |_| nav.get_value()("/settings", NavigateOptions::default())
                            }
                        >
                            "Configure System ->"
                        </button>
                    </div>
                </div>
            </div>
            
            <div style="position: absolute; bottom: -8rem; right: -4rem; text-align: right; color: rgba(255, 255, 255, 0.015); font-family: var(--font-heading); font-size: clamp(20rem, 40vw, 40rem); line-height: 0.8; user-select: none; pointer-events: none; font-style: italic;">
                "Ae"
            </div>
        </div>
    }
}
