use leptos::prelude::*;

// matches the tag list the "Strip hTML/GFX from Context" regex script uses
fn is_allowed_tag(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "div" | "span" | "p" | "strong" | "b" | "em" | "i" | "u" | "small"
            | "font" | "br" | "h1" | "h2" | "h3" | "a" | "img"
    )
}

// only absolute http(s), no javascript:/data:/relative paths
fn sanitize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

// no url()-based props (background-image etc could exfiltrate/track), and
// sanitize_style below also blocks "url(" in any value as a backstop
const ALLOWED_STYLE_PROPS: &[&str] = &[
    "color", "background", "background-color", "border", "border-color",
    "border-radius", "border-style", "border-width", "padding", "margin",
    "font-family", "font-size", "font-weight", "font-style", "text-align",
    "text-decoration", "text-transform", "white-space", "line-height",
    "letter-spacing", "display", "width", "max-width", "min-width",
    "height", "max-height", "opacity", "box-shadow", "text-shadow",
    "vertical-align", "overflow", "overflow-wrap", "word-break",
];

fn sanitize_style(raw: &str) -> String {
    raw.split(';')
        .filter_map(|decl| {
            let mut parts = decl.splitn(2, ':');
            let prop = parts.next()?.trim().to_ascii_lowercase();
            let value = parts.next()?.trim();
            if value.is_empty() || !ALLOWED_STYLE_PROPS.contains(&prop.as_str()) {
                return None;
            }
            let lower_value = value.to_ascii_lowercase();
            if lower_value.contains("url(")
                || lower_value.contains("expression(")
                || lower_value.contains("javascript:")
                || lower_value.contains("@import")
                || value.contains('<')
                || value.contains('>')
            {
                return None;
            }
            Some(format!("{prop}: {value}"))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

// tag name, byte offset right after the closing >, self-closing or not.
// tracks quotes so a stray > inside style="..." doesn't end the tag early
fn parse_open_tag(src: &str) -> Option<(String, usize, bool)> {
    let bytes = src.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut i = 1;
    let name_start = i;
    while i < bytes.len() && (bytes[i] as char).is_ascii_alphanumeric() {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = src[name_start..i].to_ascii_lowercase();
    let mut in_quote: Option<char> = None;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match in_quote {
            Some(q) => {
                if c == q {
                    in_quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_quote = Some(c);
                } else if c == '>' {
                    let self_close = i > 0 && bytes[i - 1] as char == '/';
                    return Some((name, i + 1, self_close));
                }
            }
        }
        i += 1;
    }
    None
}

fn extract_attr(open_tag_src: &str, attr_name: &str) -> Option<String> {
    let lower = open_tag_src.to_ascii_lowercase();
    let mut search_from = 0;
    loop {
        let idx = lower[search_from..].find(attr_name)?;
        let abs = search_from + idx;
        let boundary_ok = abs == 0 || !lower.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let after = &open_tag_src[abs + attr_name.len()..];
        let after_trim = after.trim_start();
        if boundary_ok && after_trim.starts_with('=') {
            let after_eq = after_trim[1..].trim_start();
            let quote = after_eq.chars().next()?;
            if quote == '"' || quote == '\'' {
                let value_src = &after_eq[1..];
                let end = value_src.find(quote)?;
                return Some(value_src[..end].to_string());
            }
        }
        search_from = abs + attr_name.len();
        if search_from >= lower.len() {
            return None;
        }
    }
}

/// old-school color="..." attrs (SillyTavern presets love these on font)
/// only need a plain css color value, so keep the charset tight instead of
/// reusing the style="..." sanitizer
fn sanitize_color_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let safe_charset = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | '%' | ' ' | '-'));
    if !safe_charset {
        return None;
    }
    Some(trimmed.to_string())
}

// legacy color="..." becomes color: ..., style="..." wins if both present
fn resolve_style(open_tag_src: &str) -> String {
    let style_attr = extract_attr(open_tag_src, "style")
        .map(|s| sanitize_style(&s))
        .unwrap_or_default();
    let color_attr = extract_attr(open_tag_src, "color").and_then(|c| sanitize_color_value(&c));
    match color_attr {
        Some(c) => {
            let base = format!("color: {c}");
            if style_attr.is_empty() {
                base
            } else {
                format!("{base}; {style_attr}")
            }
        }
        None => style_attr,
    }
}

/// `href`/`src` are already sanitized (http(s) only) or `None`.
/// `forbid_media` works like the markdown image setting: flip it and a
/// raw `<img>` just doesn't render.
fn render_element(name: &str, style: &str, children: Vec<AnyView>, href: Option<String>, src: Option<String>, forbid_media: bool) -> AnyView {
    let style = style.to_string();
    match name {
        "div" => view! { <div style=style>{children}</div> }.into_any(),
        "span" | "font" => view! { <span style=style>{children}</span> }.into_any(),
        "p" => view! { <p style=style>{children}</p> }.into_any(),
        "strong" | "b" => view! { <strong style=style>{children}</strong> }.into_any(),
        "em" | "i" => view! { <em style=style>{children}</em> }.into_any(),
        "u" => view! { <u style=style>{children}</u> }.into_any(),
        "small" => view! { <small style=style>{children}</small> }.into_any(),
        "h1" => view! { <h1 style=style>{children}</h1> }.into_any(),
        "h2" => view! { <h2 style=style>{children}</h2> }.into_any(),
        "h3" => view! { <h3 style=style>{children}</h3> }.into_any(),
        "a" => match href {
            Some(href) => view! {
                <a href=href style=style target="_blank" rel="noopener noreferrer">{children}</a>
            }.into_any(),
            // an unsafe/relative href: still show the content, just not as a link.
            None => view! { <span style=style>{children}</span> }.into_any(),
        },
        "img" => {
            if forbid_media {
                view! {}.into_any()
            } else {
                match src {
                    Some(src) => {
                        let proxy_url = format!("/api/proxy?url={}", urlencoding::encode(&src));
                        // unlike markdown image syntax, a card's raw <img> tag
                        // rarely sets its own width constraint, so without a
                        // default it renders at native resolution and
                        // overflows the content column.
                        let img_style = format!("max-width: 100%; {style}");
                        view! { <img src=proxy_url style=img_style alt="image" /> }.into_any()
                    }
                    None => view! {}.into_any(),
                }
            }
        }
        _ => view! { <span style=style>{children}</span> }.into_any(),
    }
}

// text + allowed html elements until closing tag or end of input
fn parse_children(src: &str, mut pos: usize, closing: &str, forbid_media: bool) -> (Vec<AnyView>, usize) {
    let mut out = Vec::new();
    let mut text = String::new();
    loop {
        if pos >= src.len() {
            break;
        }
        let rest = &src[pos..];
        if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                pos += end + 3;
            } else {
                pos = src.len();
            }
            continue;
        }
        if rest.starts_with("</") {
            if let Some(gt) = rest.find('>') {
                let name = rest[2..gt].trim();
                pos += gt + 1;
                if name.eq_ignore_ascii_case(closing) {
                    break;
                }
                // mismatched/unexpected closing tag, skip it defensively.
                continue;
            } else {
                pos = src.len();
                break;
            }
        }
        if rest.starts_with('<') {
            if let Some((name, attrs_end, self_close)) = parse_open_tag(rest) {
                if is_allowed_tag(&name) {
                    if !text.is_empty() {
                        out.push(view! { {decode_entities(&text)} }.into_any());
                        text.clear();
                    }
                    let style = resolve_style(&rest[..attrs_end]);
                    let href = if name == "a" {
                        extract_attr(&rest[..attrs_end], "href").and_then(|h| sanitize_url(&h))
                    } else {
                        None
                    };
                    let src_attr = if name == "img" {
                        extract_attr(&rest[..attrs_end], "src").and_then(|s| sanitize_url(&s))
                    } else {
                        None
                    };
                    let tag_end = pos + attrs_end;
                    if self_close || name == "br" || name == "img" {
                        out.push(render_element(&name, &style, Vec::new(), href, src_attr, forbid_media));
                        pos = tag_end;
                    } else {
                        let (children, new_pos) = parse_children(src, tag_end, &name, forbid_media);
                        out.push(render_element(&name, &style, children, href, src_attr, forbid_media));
                        pos = new_pos;
                    }
                    continue;
                }
            }
            // not a recognized tag: keep the '<' as literal text.
            text.push('<');
            pos += 1;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        text.push(ch);
        pos += ch.len_utf8();
    }
    if !text.is_empty() {
        out.push(view! { {decode_entities(&text)} }.into_any());
    }
    (out, pos)
}

pub enum TextOrHtml<'a> {
    Text(&'a str),
    Html(AnyView),
}

// pulled out as real elements even at top level, outside paragraphs
const TOP_LEVEL_TAGS: &[&str] = &["div", "span", "font", "a", "img"];

// pulls top-level allowed html out as sanitized elements, rest is plain
// markdown text
pub fn extract_html_segments(text: &str, forbid_media: bool) -> Vec<TextOrHtml<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        match find_next_top_level_tag(text, pos) {
            None => {
                if pos < text.len() {
                    out.push(TextOrHtml::Text(&text[pos..]));
                }
                break;
            }
            Some((start, name, attrs_end, self_close)) => {
                if start > pos {
                    out.push(TextOrHtml::Text(&text[pos..start]));
                }
                let open_tag_src = &text[start..start + attrs_end];
                let style = resolve_style(open_tag_src);
                let href = if name == "a" { extract_attr(open_tag_src, "href").and_then(|h| sanitize_url(&h)) } else { None };
                let src_attr = if name == "img" { extract_attr(open_tag_src, "src").and_then(|s| sanitize_url(&s)) } else { None };
                if self_close || name == "img" {
                    out.push(TextOrHtml::Html(render_element(&name, &style, Vec::new(), href, src_attr, forbid_media)));
                    pos = start + attrs_end;
                } else {
                    let (children, end_pos) = parse_children(text, start + attrs_end, &name, forbid_media);
                    out.push(TextOrHtml::Html(render_element(&name, &style, children, href, src_attr, forbid_media)));
                    pos = end_pos;
                }
            }
        }
    }
    out
}

fn find_next_top_level_tag(text: &str, from: usize) -> Option<(usize, String, usize, bool)> {
    let lower = text.to_ascii_lowercase();
    let mut best: Option<(usize, String, usize, bool)> = None;
    for tag in TOP_LEVEL_TAGS {
        let needle = format!("<{tag}");
        let mut search_from = from;
        while let Some(rel) = lower[search_from..].find(&needle) {
            let start = search_from + rel;
            let after = text[start + needle.len()..].chars().next();
            let boundary_ok = matches!(after, Some(c) if c.is_whitespace() || c == '>' || c == '/');
            if boundary_ok {
                if let Some((name, attrs_end, self_close)) = parse_open_tag(&text[start..]) {
                    if best.as_ref().map_or(true, |(b_start, ..)| start < *b_start) {
                        best = Some((start, name, attrs_end, self_close));
                    }
                    break;
                }
            }
            search_from = start + needle.len();
        }
    }
    best
}

fn is_plot_momentum_summary(block_lower: &str) -> bool {
    let Some(sum_start) = block_lower.find("<summary") else { return false; };
    let Some(gt) = block_lower[sum_start..].find('>') else { return false; };
    let content_start = sum_start + gt + 1;
    let Some(close_rel) = block_lower[content_start..].find("</summary>") else { return false; };
    block_lower[content_start..content_start + close_rel].contains("plot momentum")
}

// strips plot momentum <details> blocks from display (preset's own
// planning notes, not meant to be read). other details blocks untouched
pub fn strip_plot_momentum(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    loop {
        let Some(rel) = lower[pos..].find("<details") else {
            out.push_str(&text[pos..]);
            break;
        };
        let start = pos + rel;
        out.push_str(&text[pos..start]);

        let mut depth = 1;
        let mut scan = start + "<details".len();
        let mut end_of_block = None;
        loop {
            let next_open = lower[scan..].find("<details").map(|i| scan + i);
            let next_close = lower[scan..].find("</details>").map(|i| scan + i);
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    depth += 1;
                    scan = o + "<details".len();
                }
                (_, Some(c)) => {
                    depth -= 1;
                    scan = c + "</details>".len();
                    if depth == 0 {
                        end_of_block = Some(scan);
                        break;
                    }
                }
                _ => break,
            }
        }

        match end_of_block {
            Some(end) => {
                let block = &text[start..end];
                if !is_plot_momentum_summary(&lower[start..end]) {
                    out.push_str(block);
                }
                pos = end;
            }
            None => {
                out.push_str(&text[start..]);
                pos = text.len();
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{sanitize_style, sanitize_url, strip_plot_momentum};

    #[test]
    fn sanitize_url_keeps_http_and_https() {
        assert_eq!(sanitize_url("https://example.com/a.png"), Some("https://example.com/a.png".to_string()));
        assert_eq!(sanitize_url("http://example.com/a.png"), Some("http://example.com/a.png".to_string()));
    }

    #[test]
    fn sanitize_url_rejects_javascript_scheme() {
        assert_eq!(sanitize_url("javascript:alert(1)"), None);
    }

    #[test]
    fn sanitize_url_rejects_data_scheme() {
        assert_eq!(sanitize_url("data:text/html,<script>alert(1)</script>"), None);
    }

    #[test]
    fn sanitize_url_rejects_relative_paths() {
        assert_eq!(sanitize_url("/tags/some-tag"), None);
    }

    #[test]
    fn keeps_allowlisted_properties() {
        let out = sanitize_style("color: #ff0000; background: black; padding: 8px");
        assert_eq!(out, "color: #ff0000; background: black; padding: 8px");
    }

    #[test]
    fn drops_url_based_values() {
        let out = sanitize_style("background: url(https://evil.example/track.png); color: red");
        assert_eq!(out, "color: red");
    }

    #[test]
    fn drops_disallowed_properties() {
        let out = sanitize_style("position: fixed; color: red");
        assert_eq!(out, "color: red");
    }

    #[test]
    fn strips_plot_momentum_block_and_keeps_surrounding_text() {
        let text = "*She smiles.* \"Good morning.\"\n\n<details><summary>Plot Momentum</summary>\nNext beat: she reveals the letter.\n</details>";
        let out = strip_plot_momentum(text);
        assert_eq!(out, "*She smiles.* \"Good morning.\"\n\n");
    }

    #[test]
    fn leaves_unrelated_details_blocks_alone() {
        let text = "<details><summary>Spoiler</summary>plot twist here</details>";
        let out = strip_plot_momentum(text);
        assert_eq!(out, text);
    }
}
