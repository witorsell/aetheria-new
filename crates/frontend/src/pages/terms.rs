use leptos::prelude::*;

#[component]
pub fn TermsPage() -> impl IntoView {
    view! {
        <div class="legal-page" style="color: var(--color-text);">
            <p style="margin-bottom: 2rem;">
                <a href="/" style="color: var(--color-text-muted); text-decoration: none; font-family: monospace; text-transform: uppercase; letter-spacing: 0.1em; font-size: 0.8rem; border-bottom: 1px dotted var(--color-border); padding-bottom: 2px;">
                    "< BACK"
                </a>
            </p>

            <div style="margin-bottom: 3rem;">
                <h1 class="legal-page-title" style="font-family: var(--font-heading); font-weight: 300; margin: 0; color: #fff; letter-spacing: -0.02em;">"Terms of Service"</h1>
                <p style="color: var(--color-text-muted); font-family: monospace; margin-top: 0.5rem; text-transform: uppercase; font-size: 0.85rem; letter-spacing: 0.05em;">"Last updated 2026"</p>
            </div>

            <div style="display: flex; flex-direction: column; gap: 2rem; line-height: 1.7; color: #cfcfd6; font-family: var(--font-body); margin-bottom: 5rem;">
                <p>
                    "Aetheria is a privately run, single-operator chat application. It is not offered as a public
                    product or service, has no signup flow, and is not intended for use by anyone other than the
                    people its operator explicitly gives an account to."
                </p>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"What this app does"</h2>
                    <p>
                        "Aetheria assembles prompts from the characters, lorebooks, presets, and settings you configure,
                        and forwards them to whichever AI model provider you've entered your own API credentials for
                        (in Settings). It does not run its own model and does not generate content on its own."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"No warranty"</h2>
                    <p>
                        "This software is provided as-is, without warranty of any kind. It may contain bugs,
                        may lose data, and may behave unexpectedly. The operator makes no guarantee of uptime,
                        availability, or fitness for any particular purpose."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"Your responsibility"</h2>
                    <p>
                        "You are responsible for the content you generate and store here, for keeping your account
                        credentials private, and for complying with the terms of service of whichever model provider
                        you connect through your own API key."
                    </p>
                </div>

                <div>
                    <h2 style="font-family: var(--font-heading); font-weight: 400; font-size: 1.4rem; color: #fff; margin-bottom: 0.75rem;">"Changes"</h2>
                    <p>
                        "These terms may change at any time without notice, since this is a personal project rather
                        than a maintained public service."
                    </p>
                </div>
            </div>
        </div>
    }
}
