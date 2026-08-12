use crate::api::ThemeTokens;
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct ThemeStore(pub RwSignal<ThemeTokens>);

/// writes every token onto `:root` as a CSS custom property, and toggles the
/// handful of tokens that are shape/mode switches rather than raw CSS
/// values as body classes, mirroring how SillyTavern's `applyTheme()`
/// handles `avatar_style`/`chat_display` via body classes instead of custom
/// properties (see `SillyTavern`'s `power-user.js`).
pub fn apply_tokens_to_root(tokens: &ThemeTokens) {
    use wasm_bindgen::JsCast;

    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Some(root) = document.document_element() else { return };
    let Ok(html_el) = root.dyn_into::<web_sys::HtmlElement>() else { return };
    let style = html_el.style();

    let _ = style.set_property("--color-bg", &tokens.color_bg);
    let _ = style.set_property("--color-surface", &tokens.color_surface);
    let _ = style.set_property("--color-surface-hover", &tokens.color_surface_hover);
    let _ = style.set_property("--color-border", &tokens.color_border);
    let _ = style.set_property("--color-accent", &tokens.color_accent);
    let _ = style.set_property("--color-accent-2", &tokens.color_accent_2);
    let _ = style.set_property("--color-accent-hover", &tokens.color_accent_hover);
    let _ = style.set_property("--color-text", &tokens.color_text);
    let _ = style.set_property("--color-text-muted", &tokens.color_text_muted);
    let _ = style.set_property("--color-text-heading", &tokens.color_text_heading);
    let _ = style.set_property("--color-error", &tokens.color_error);
    let _ = style.set_property("--color-error-bg", &tokens.color_error_bg);
    let _ = style.set_property("--font-heading", &tokens.font_heading);
    let _ = style.set_property("--font-body", &tokens.font_body);
    let _ = style.set_property("--font-scale", &tokens.font_scale.to_string());
    let _ = style.set_property("--radius-sm", &tokens.radius_sm);
    let _ = style.set_property("--radius-md", &tokens.radius_md);
    let _ = style.set_property("--radius-lg", &tokens.radius_lg);
    let _ = style.set_property("--blur-strength", &format!("{}px", tokens.blur_strength));
    let _ = style.set_property("--shadow-strength", &tokens.shadow_strength.to_string());
    let _ = style.set_property("--chat-width", &format!("{}vw", tokens.chat_width));
    let _ = style.set_property("--mascot-accent", &tokens.mascot_accent);

    let Some(body) = document.body() else { return };
    for class in ["avatar-circle", "avatar-rounded", "avatar-square"] {
        let _ = body.class_list().remove_1(class);
    }
    let _ = body.class_list().add_1(&format!("avatar-{}", tokens.avatar_style));
    for class in ["chat-bubble", "chat-flat"] {
        let _ = body.class_list().remove_1(class);
    }
    let _ = body.class_list().add_1(&format!("chat-{}", tokens.chat_display));
    let _ = body.class_list().toggle_with_force("reduced-motion", tokens.reduced_motion);
    let _ = body.class_list().toggle_with_force("mascot-disabled", !tokens.mascot_enabled);

    // custom_css: replace (not append to) a single <style id="theme-custom-css">
    // element each time, so switching themes doesn't leak the old theme's rules
    if let Ok(Some(existing)) = document.query_selector("#theme-custom-css") {
        existing.remove();
    }
    if !tokens.custom_css.is_empty() {
        if let Ok(style_el) = document.create_element("style") {
            let _ = style_el.set_attribute("id", "theme-custom-css");
            style_el.set_text_content(Some(&tokens.custom_css));
            if let Some(head) = document.head() {
                let _ = head.append_child(&style_el);
            }
        }
    }
}
