use leptos::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// pushes a completed block-level view (a `<p>`, `<h1>`..`<h6>`, or
/// `<pre><code>`) to wherever it belongs: into the block list of the
/// currently open list item, if we're nested inside one, or onto the
/// top-level `blocks` list otherwise. without this, a paragraph or code
/// block that pulldown-cmark nests inside a list item (which happens for
/// any "loose" list, i.e. one with a blank line between items, and for any
/// list item containing a fenced code block) would land as a stray
/// top-level block instead of inside its `<li>`.
fn push_block(view: AnyView, item_stack: &mut [Vec<AnyView>], blocks: &mut Vec<AnyView>) {
    if let Some(current_item) = item_stack.last_mut() {
        current_item.push(view);
    } else {
        blocks.push(view);
    }
}

/// replaces {{char}}/{{name}} with the character's name and {{user}} with
/// the logged-in user's name, matching old aetheria's macro convention.
/// runs before markdown parsing.
fn substitute_macros(text: &str, char_name: &str, user_name: &str) -> String {
    text.replace("{{char}}", char_name)
        .replace("{{Char}}", char_name)
        .replace("{char}", char_name)
        .replace("{Char}", char_name)
        .replace("{{name}}", char_name)
        .replace("{{user}}", user_name)
        .replace("{{User}}", user_name)
        .replace("{user}", user_name)
        .replace("{User}", user_name)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = katex, catch)]
    fn renderToString(math: &str, options: &wasm_bindgen::JsValue) -> Result<String, wasm_bindgen::JsValue>;
}

fn render_math_to_html(math: &str, display_mode: bool) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&options, &"displayMode".into(), &display_mode.into());
        let _ = js_sys::Reflect::set(&options, &"throwOnError".into(), &false.into());
        
        if let Ok(html) = renderToString(math, &options) {
            return html;
        }
    }
    
    // fallback if not wasm32 or if katex failed
    let escaped = html_escape::encode_text(math);
    if display_mode {
        format!("<div class=\"math math-display\" style=\"text-align: center; margin: 1em 0;\">{}</div>", escaped)
    } else {
        format!("<span class=\"math math-inline\">{}</span>", escaped)
    }
}

/// if `text` starts (after leading whitespace) with a single backtick code
/// span whose entire content is a `[...]` bracket, like a preset's
/// `` `[ 🕰️ morning, day 3 - the cafe ]` `` time/place header, returns the
/// bracket's inner text and everything after the closing backtick. only
/// fires at the very start of the message, matching the "one header per
/// response" convention.
fn extract_leading_time_header(text: &str) -> Option<(String, &str)> {
    let trimmed = text.trim_start();
    let after_tick = trimmed.strip_prefix('`')?;
    let after_bracket_open = after_tick.strip_prefix('[')?;
    let close = after_bracket_open.find(']')?;
    let after_bracket_close = &after_bracket_open[close + 1..];
    let remainder = after_bracket_close.strip_prefix('`')?;
    let header = after_bracket_open[..close].trim().to_string();
    Some((header, remainder))
}

fn render_time_header(header: &str) -> AnyView {
    view! {
        <div style="display: inline-flex; align-items: center; padding: 3px 12px; margin-bottom: 8px; border: 1px solid var(--color-border); border-radius: 999px; font-family: monospace; font-size: 0.8rem; letter-spacing: 0.02em; color: var(--color-text-muted); background: rgba(255, 255, 255, 0.03);">
            {header.to_string()}
        </div>
    }.into_any()
}

/// markdown -> block-level Leptos views, plus {{char}}/{{user}} macros and
/// the quoted-speech/emphasis styling. no raw inner_html injection: the
/// allowlisted subset from safe_html (GFX divs, font/span colors) gets
/// pulled out as real sanitized elements first, everything else is plain
/// pulldown-cmark and any other raw html just gets dropped. leading
/// backtick `[...]` time/place header gets its own styled block instead of
/// looking like inline code.
pub fn render_markdown(text: &str, char_name: &str, user_name: &str, forbid_media: bool) -> Vec<AnyView> {
    let substituted = substitute_macros(text, char_name, user_name);
    let stripped = crate::render::safe_html::strip_plot_momentum(&substituted);
    let mut blocks: Vec<AnyView> = Vec::new();
    let body: &str = match extract_leading_time_header(&stripped) {
        Some((header, remainder)) => {
            blocks.push(render_time_header(&header));
            remainder
        }
        None => &stripped,
    };
    for segment in crate::render::safe_html::extract_html_segments(body, forbid_media) {
        match segment {
            crate::render::safe_html::TextOrHtml::Text(md) => {
                render_markdown_blocks(md, &mut blocks, forbid_media);
            }
            crate::render::safe_html::TextOrHtml::Html(view) => blocks.push(view),
        }
    }
    blocks
}

fn render_markdown_blocks(text: &str, blocks: &mut Vec<AnyView>, forbid_media: bool) {
    let mut options = Options::ENABLE_STRIKETHROUGH;
    options.insert(Options::ENABLE_MATH);
    let parser = Parser::new_ext(text, options);

    let mut inline: Vec<AnyView> = Vec::new();
    let mut in_emphasis = false;
    let mut in_quote = false;
    let mut code_block_buffer: Option<String> = None;
    let mut image_url: Option<String> = None;
    let mut image_title: Option<String> = None;
    let mut image_inline_start: Option<usize> = None;
    let mut link_url: Option<String> = None;
    let mut link_title: Option<String> = None;
    let mut link_inline_start: Option<usize> = None;
    // one entry per currently-open list, its <li> views so far. nested lists
    // need a stack here not a flat vec so closing one drops into the right parent
    let mut list_stack: Vec<Vec<AnyView>> = Vec::new();
    let mut list_type_stack: Vec<Option<u64>> = Vec::new();
    // each stack entry is the currently-open list item's own block-level
    // views (paragraphs, code blocks, nested lists). a stack so a list
    // nested inside a list item closes into that item, not the outer one.
    let mut item_stack: Vec<Vec<AnyView>> = Vec::new();

    for event in parser {
        match event {
            Event::Start(Tag::Paragraph) => inline.clear(),
            Event::End(TagEnd::Paragraph) => {
                let view = view! { <p>{inline.drain(..).collect_view()}</p> }.into_any();
                push_block(view, &mut item_stack, blocks);
            }
            Event::Start(Tag::Heading { .. }) => inline.clear(),
            Event::End(TagEnd::Heading(level)) => {
                let children = inline.drain(..).collect_view();
                let view = match level {
                    HeadingLevel::H1 => view! { <h1>{children}</h1> }.into_any(),
                    HeadingLevel::H2 => view! { <h2>{children}</h2> }.into_any(),
                    HeadingLevel::H3 => view! { <h3>{children}</h3> }.into_any(),
                    HeadingLevel::H4 => view! { <h4>{children}</h4> }.into_any(),
                    HeadingLevel::H5 => view! { <h5>{children}</h5> }.into_any(),
                    HeadingLevel::H6 => view! { <h6>{children}</h6> }.into_any(),
                };
                push_block(view, &mut item_stack, blocks);
            }
            Event::Start(Tag::Emphasis) => in_emphasis = true,
            Event::End(TagEnd::Emphasis) => in_emphasis = false,
            Event::Start(Tag::Strong) => {}
            Event::End(TagEnd::Strong) => {}
            Event::Start(Tag::CodeBlock(_)) => code_block_buffer = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(code) = code_block_buffer.take() {
                    let view = view! { <pre><code>{code}</code></pre> }.into_any();
                    push_block(view, &mut item_stack, blocks);
                }
            }
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                image_url = Some(dest_url.to_string());
                image_title = Some(title.to_string());
                image_inline_start = Some(inline.len());
            }
            Event::End(TagEnd::Image) => {
                if let Some(start_len) = image_inline_start.take() {
                    // drop the alt text that was added to inline
                    inline.truncate(start_len);
                }
                if let Some(url) = image_url.take() {
                    if !forbid_media {
                        let title = image_title.take().unwrap_or_default();
                        let proxy_url = format!("/api/proxy?url={}", urlencoding::encode(&url));
                        let view = view! {
                            <crate::components::proxied_image::ProxiedImage
                                src=proxy_url
                                title=title
                                alt="image"
                                style="max-width: 100%; border-radius: 4px; margin: 8px 0; display: block;"
                            />
                        }.into_any();
                        inline.push(view);
                    }
                }
            }
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                // same sanitize_url used for raw <a href> in safe_html.rs - markdown link
                // syntax was going straight to href with no scheme check, a javascript:
                // link in a shared/imported character card. the CSP (script-src 'self'
                // 'wasm-unsafe-eval', no unsafe-inline) already blocks javascript: nav
                // in Chromium-family browsers as a second layer, but fix it at the source.
                link_url = crate::render::safe_html::sanitize_url(&dest_url);
                link_title = Some(title.to_string());
                link_inline_start = Some(inline.len());
            }
            Event::End(TagEnd::Link) => {
                if let Some(start_len) = link_inline_start.take() {
                    let children = inline.drain(start_len..).collect_view();
                    let title = link_title.take().unwrap_or_default();
                    let view = match link_url.take() {
                        Some(url) => view! {
                            <a href=url title=title target="_blank" rel="noopener noreferrer" style="color: #00DFD8; text-decoration: underline;">
                                {children}
                            </a>
                        }.into_any(),
                        // an unsafe/relative href: still show the content, just not as a link.
                        None => view! { <span>{children}</span> }.into_any(),
                    };
                    inline.push(view);
                }
            }
            Event::Start(Tag::List(start_num)) => {
                list_stack.push(Vec::new());
                list_type_stack.push(start_num);
            },
            Event::End(TagEnd::List(_)) => {
                let items = list_stack.pop().unwrap_or_default();
                let start_num = list_type_stack.pop().unwrap_or(None);
                let view = if let Some(n) = start_num {
                    view! { <ol start=n as i32>{items}</ol> }.into_any()
                } else {
                    view! { <ul>{items}</ul> }.into_any()
                };
                push_block(view, &mut item_stack, blocks);
            }
            Event::Start(Tag::Item) => {
                inline.clear();
                item_stack.push(Vec::new());
            }
            Event::End(TagEnd::Item) => {
                // tight list items (the common case, e.g. `- one\n- two`)
                // never get a nested paragraph event, their text arrives
                // directly as text events into `inline`. flush whatever's
                // left there into the item's own block buffer before
                // closing it, so tight items keep rendering without a
                // wrapping `<p>` while loose items (whose text already
                // went through the paragraph end handler above) are
                // unaffected here.
                if !inline.is_empty() {
                    if let Some(current_item) = item_stack.last_mut() {
                        current_item.extend(inline.drain(..));
                    }
                }
                let item_blocks = item_stack.pop().unwrap_or_default();
                let li = view! { <li>{item_blocks}</li> }.into_any();
                if let Some(current_list) = list_stack.last_mut() {
                    current_list.push(li);
                }
            }
            Event::Text(text) => {
                if let Some(buffer) = code_block_buffer.as_mut() {
                    buffer.push_str(&text);
                    continue;
                }
                
                let mut segment_start = 0;
                let mut chars = text.char_indices().peekable();
                
                let mut push_segment = |segment: &str, is_quoted: bool, is_emph: bool| {
                    if segment.is_empty() { return; }
                    let view = if is_quoted {
                        if is_emph {
                            view! { <span class="message-qem">{segment.to_string()}</span> }.into_any()
                        } else {
                            view! { <span class="message-quote">{segment.to_string()}</span> }.into_any()
                        }
                    } else if is_emph {
                        view! { <em>{segment.to_string()}</em> }.into_any()
                    } else {
                        view! { {segment.to_string()} }.into_any()
                    };
                    inline.push(view);
                };

                while let Some((idx, ch)) = chars.next() {
                    if ch == '"' {
                        if !in_quote {
                            if idx > segment_start {
                                push_segment(&text[segment_start..idx], false, in_emphasis);
                            }
                            in_quote = true;
                            segment_start = idx;
                        } else {
                            push_segment(&text[segment_start..=idx], true, in_emphasis);
                            in_quote = false;
                            segment_start = idx + 1;
                        }
                    }
                }
                if segment_start < text.len() {
                    push_segment(&text[segment_start..], in_quote, in_emphasis);
                }
            }
            Event::Code(code) => {
                inline.push(view! { <code>{code.to_string()}</code> }.into_any());
            }
            Event::InlineMath(math) => {
                let html = render_math_to_html(&math, false);
                inline.push(view! { <span inner_html=html></span> }.into_any());
            }
            Event::DisplayMath(math) => {
                let html = render_math_to_html(&math, true);
                let view = view! { <div inner_html=html></div> }.into_any();
                push_block(view, &mut item_stack, blocks);
            }
            Event::SoftBreak => inline.push(view! { <br /> }.into_any()),
            Event::HardBreak => inline.push(view! { <br /> }.into_any()),
            _ => {}
        }
    }

    // by this point every Start/End pair should be balanced and item_stack
    // empty, but push_block is used defensively rather than assuming that.
    if !inline.is_empty() {
        let view = view! { <p>{inline.drain(..).collect_view()}</p> }.into_any();
        push_block(view, &mut item_stack, blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_char_and_user_macros() {
        let result = substitute_macros("{{char}} looks at {{user}}.", "Seraphina", "Testuser");
        assert_eq!(result, "Seraphina looks at Testuser.");
    }

    #[test]
    fn name_macro_is_an_alias_for_char() {
        let result = substitute_macros("{{name}} nods.", "Seraphina", "Testuser");
        assert_eq!(result, "Seraphina nods.");
    }

    #[test]
    fn extracts_leading_time_header() {
        let (header, rest) = extract_leading_time_header(
            "`[ 🕰️ Morning, Day 3 - The Whispering Cafe ]`\n\n*She looks up.*",
        )
        .expect("should detect the leading header");
        assert_eq!(header, "🕰️ Morning, Day 3 - The Whispering Cafe");
        assert_eq!(rest, "\n\n*She looks up.*");
    }

    #[test]
    fn does_not_treat_a_regular_code_span_as_a_time_header() {
        assert!(extract_leading_time_header("`let x = 1;` is code.").is_none());
    }

    #[test]
    fn does_not_treat_a_mid_message_bracket_span_as_a_header() {
        assert!(extract_leading_time_header("She said `[laughs]` and left.").is_none());
    }
}



// `render_markdown` returns real Leptos views (backed by actual DOM nodes
// under the `csr` feature), so it can't be exercised by a plain native
// `cargo test`: constructing a `<div>` etc. calls into web-sys/wasm-bindgen
// DOM bindings that only work when actually running as wasm inside a
// browser. these tests run under `wasm-bindgen-test` in headless chrome
// (via chromedriver), mount the real output into a detached DOM node, and
// assert on the resulting DOM shape, proving the two bugs found in review
// (heading text silently dropped by the shared `inline` buffer, and list
// items with nested paragraphs/code blocks losing their place in the list)
// are fixed against the actual renderer, not just by reading the code.
//
// run with:
//   cd crates/frontend
//   CHROMEDRIVER=/usr/bin/chromedriver CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//     cargo test --target wasm32-unknown-unknown --bin frontend render::markdown::dom_tests
#[cfg(all(test, target_arch = "wasm32"))]
mod dom_tests {
    use super::*;
    use wasm_bindgen::JsCast;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    /// mounts `render_markdown`'s output into a fresh, detached `<div>` and
    /// hands back that container so the test can inspect the real DOM tree.
    fn mount(markdown: &str) -> web_sys::HtmlElement {
        let container: web_sys::HtmlElement = document()
            .create_element("div")
            .expect("create_element should succeed")
            .dyn_into()
            .expect("div is an HtmlElement");
        let blocks = render_markdown(markdown, "Seraphina", "Testuser", false);
        // leak the mount handle: the container is dropped at the end of the
        // test process anyway, and we only need the DOM state it produced.
        std::mem::forget(mount_to(container.clone(), move || blocks));
        container
    }

    #[wasm_bindgen_test]
    fn heading_text_survives_between_paragraphs() {
        let container = mount("Before paragraph.\n\n# A Heading\n\nAfter paragraph.");

        // all three blocks must be present as their own top-level element,
        // the bug wiped the heading's text via the next block's `inline.clear()`.
        assert_eq!(
            container.child_element_count(),
            3,
            "expected 3 top-level blocks (p, h1, p), got: {}",
            container.inner_html()
        );

        let text = container.text_content().unwrap_or_default();
        assert!(text.contains("Before paragraph."), "got: {text}");
        assert!(text.contains("A Heading"), "heading text was dropped, got: {text}");
        assert!(text.contains("After paragraph."), "got: {text}");

        let heading = container
            .query_selector("h1")
            .unwrap()
            .expect("a real <h1> should have been rendered, not just a <p>");
        assert_eq!(heading.text_content().unwrap_or_default(), "A Heading");
    }

    #[wasm_bindgen_test]
    fn loose_list_items_keep_their_paragraph_inside_the_li() {
        // a blank line between items makes this a "loose" list in
        // CommonMark, so pulldown-cmark wraps each item's text in its own
        // nested paragraph event instead of emitting text directly.
        let container = mount("- one\n\n- two\n");

        // nothing should have leaked out as a sibling of the <ul>.
        assert_eq!(
            container.child_element_count(),
            1,
            "expected only the <ul> at the top level, got: {}",
            container.inner_html()
        );

        let list = container
            .query_selector("ul")
            .unwrap()
            .expect("a <ul> should have been rendered");
        let items = list.query_selector_all("li").unwrap();
        assert_eq!(items.length(), 2, "expected 2 <li> items, got: {}", list.inner_html());

        let first_item_text = items
            .get(0)
            .unwrap()
            .dyn_into::<web_sys::Element>()
            .unwrap()
            .text_content()
            .unwrap_or_default();
        assert!(
            first_item_text.contains("one"),
            "first <li> should contain its own text, got: {}",
            list.inner_html()
        );
        let second_item_text = items
            .get(1)
            .unwrap()
            .dyn_into::<web_sys::Element>()
            .unwrap()
            .text_content()
            .unwrap_or_default();
        assert!(
            second_item_text.contains("two"),
            "second <li> should contain its own text, got: {}",
            list.inner_html()
        );
    }

    #[wasm_bindgen_test]
    fn list_item_with_a_fenced_code_block_stays_inside_the_li() {
        // continuation lines (blank line + 2-space indent, matching where
        // this item's content starts after "- ") keep the fenced code
        // block nested inside the single list item.
        let container = mount("- item with code:\n\n  ```\n  let x = 1;\n  ```\n");

        assert_eq!(
            container.child_element_count(),
            1,
            "expected only the <ul> at the top level, no orphaned <p>/<pre>, got: {}",
            container.inner_html()
        );

        let list = container
            .query_selector("ul")
            .unwrap()
            .expect("a <ul> should have been rendered");
        let items = list.query_selector_all("li").unwrap();
        assert_eq!(items.length(), 1, "expected exactly 1 <li>, got: {}", list.inner_html());

        let item = items.get(0).unwrap().dyn_into::<web_sys::Element>().unwrap();
        let item_text = item.text_content().unwrap_or_default();
        assert!(item_text.contains("item with code:"), "got: {item_text}");
        assert!(item_text.contains("let x = 1;"), "got: {item_text}");

        let code = item
            .query_selector("pre code")
            .unwrap()
            .expect("the fenced code block should be nested inside the <li>, not a sibling");
        assert_eq!(code.text_content().unwrap_or_default().trim(), "let x = 1;");
    }

    #[wasm_bindgen_test]
    fn a_javascript_scheme_link_renders_inert_not_as_an_anchor() {
        let container = mount("[click me](javascript:alert(1))");

        assert!(
            container.query_selector("a").unwrap().is_none(),
            "a javascript: link should not render as an <a> at all, got: {}",
            container.inner_html()
        );
        let text = container.text_content().unwrap_or_default();
        assert!(text.contains("click me"), "link text should still render, got: {text}");
    }

    #[wasm_bindgen_test]
    fn an_https_link_still_renders_as_a_real_anchor() {
        let container = mount("[click me](https://example.com/)");

        let link = container
            .query_selector("a")
            .unwrap()
            .expect("a normal https link should still render as an <a>");
        assert_eq!(link.get_attribute("href").unwrap_or_default(), "https://example.com/");
    }
}
