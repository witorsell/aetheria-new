const OPEN_TAG: &str = "<think>";
const CLOSE_TAG: &str = "</think>";

/// frontend copy of the server's `extract_thinking` (crates/server/src/reasoning.rs).
/// duplicated deliberately: this wasm crate cannot depend on the native server crate.
pub fn extract_thinking(text: &str) -> (String, Option<String>) {
    let mut visible = String::new();
    let mut thoughts: Vec<String> = Vec::new();
    let mut remaining = text;

    loop {
        let Some(open_pos) = remaining.find(OPEN_TAG) else {
            // no open tag left. a stray close tag with no matching open tag
            // (e.g. a stream that got truncated right after the model
            // started emitting `</think>`) still counts as thought content.
            if let Some(close_pos) = remaining.find(CLOSE_TAG) {
                thoughts.push(remaining[..close_pos].to_string());
                visible.push_str(&remaining[close_pos + CLOSE_TAG.len()..]);
            } else {
                visible.push_str(remaining);
            }
            break;
        };

        visible.push_str(&remaining[..open_pos]);
        let after_open = &remaining[open_pos + OPEN_TAG.len()..];

        match after_open.find(CLOSE_TAG) {
            Some(close_pos) => {
                thoughts.push(after_open[..close_pos].to_string());
                remaining = &after_open[close_pos + CLOSE_TAG.len()..];
            }
            None => {
                thoughts.push(after_open.to_string());
                break;
            }
        }
    }

    if thoughts.is_empty() {
        return (visible, None);
    }
    (visible, Some(thoughts.join("\n\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_text_unchanged_when_no_tags_present() {
        let (visible, thought) = extract_thinking("just a normal reply");
        assert_eq!(visible, "just a normal reply");
        assert_eq!(thought, None);
    }

    #[test]
    fn extracts_a_matched_pair() {
        let (visible, thought) = extract_thinking("<think>pondering</think>Hello there.");
        assert_eq!(visible, "Hello there.");
        assert_eq!(thought, Some("pondering".to_string()));
    }

    #[test]
    fn treats_an_unterminated_open_tag_as_an_in_progress_thought() {
        let (visible, thought) = extract_thinking("<think>still thinking, no close tag yet");
        assert_eq!(visible, "");
        assert_eq!(thought, Some("still thinking, no close tag yet".to_string()));
    }

    #[test]
    fn treats_an_unterminated_close_tag_as_a_thought_with_no_visible_text() {
        let (visible, thought) = extract_thinking("some thought text</think>");
        assert_eq!(visible, "");
        assert_eq!(thought, Some("some thought text".to_string()));
    }

    #[test]
    fn concatenates_multiple_thought_blocks() {
        let (visible, thought) =
            extract_thinking("<think>first</think>middle<think>second</think>end");
        assert_eq!(visible, "middleend");
        assert_eq!(thought, Some("first\n\nsecond".to_string()));
    }
}
