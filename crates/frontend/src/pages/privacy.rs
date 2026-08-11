use leptos::prelude::*;

#[component]
pub fn PrivacyPage() -> impl IntoView {
    view! {
        <div class="legal-page" style="color: var(--color-text);">
            <p style="margin-bottom: 2rem;">
                <a href="/" style="color: var(--color-text-muted); text-decoration: none; font-family: monospace; text-transform: uppercase; letter-spacing: 0.1em; font-size: 0.8rem; border-bottom: 1px dotted var(--color-border); padding-bottom: 2px;">
                    "< BACK"
                </a>
            </p>

            <div style="margin-bottom: 3rem;">
                <h1 class="legal-page-title" style="font-family: var(--font-heading); font-weight: 300; margin: 0; color: #fff; letter-spacing: -0.02em;">"Privacy Policy"</h1>
                <p style="color: var(--color-text-muted); font-family: monospace; margin-top: 0.5rem; text-transform: uppercase; font-size: 0.85rem; letter-spacing: 0.05em;">"Last updated 2026"</p>
            </div>

            <div style="display: flex; flex-direction: column; gap: 2rem; line-height: 1.7; color: #cfcfd6; font-family: var(--font-body); margin-bottom: 5rem;">
                <p>
                    "Aetheria is self-hosted and privately run. There is no analytics tracking, no advertising,
                    and no data broker relationship of any kind. Everything below describes exactly where your
                    data actually goes."
                </p>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"What's stored, and where"</h2>
                    <p>
                        "Accounts, characters, lorebooks, presets, chat messages, and uploaded avatar images are
                        stored in this app's own database and uploads folder, on the server it runs on. Nothing
                        is copied to a third-party analytics or storage service by the app itself."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"What leaves the server"</h2>
                    <p>
                        "When you generate a message, the assembled prompt (character definitions, lorebook
                        entries, chat history, and your message) is sent to the AI model provider configured in
                        your own Settings, using your own API key. That provider's own privacy policy governs
                        what happens to data once it reaches them. Aetheria does not send your data anywhere else."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"Cookies"</h2>
                    <p>
                        "A single session cookie is used to keep you logged in. It isn't used for tracking and
                        isn't shared with anyone."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"Deletion"</h2>
                    <p>
                        "Deleting a character, chat, or message removes it from the database through the app's
                        own delete actions. Since this is a personal deployment, contact the operator directly for
                        anything the UI doesn't cover, like full account removal."
                    </p>
                </div>
            </div>
        </div>
    }
}
