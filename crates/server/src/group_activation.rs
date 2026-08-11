use regex::Regex;
use std::sync::LazyLock;

static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\w+\b").expect("static regex is valid"));

/// one group member eligible to be considered for activation this turn.
/// callers pass only currently-enabled members (i.e. `group_members.disabled
/// = 0`); this module has no concept of `disabled` at all.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivationCandidate {
    pub character_id: String,
    pub name: String,
    pub talkativeness: f64,
}

/// list strategy: every candidate responds, once each, in the order given.
/// callers pass members already ordered by `group_members.position`.
pub fn activate_list(candidates: &[ActivationCandidate]) -> Vec<String> {
    candidates.iter().map(|c| c.character_id.clone()).collect()
}

/// lowercased `\b\w+\b` word tokens, matching SillyTavern's
/// `extractAllWords` (public/scripts/utils.js). the regex is compiled once
/// (see `WORD_RE`) instead of per call, since `activate_natural` re-tokenizes
/// every candidate name on every group turn now that it has a real caller.
fn extract_all_words(text: &str) -> Vec<String> {
    WORD_RE.find_iter(text).map(|m| m.as_str().to_lowercase()).collect()
}

/// natural strategy: mention detection, then a per-candidate talkativeness
/// roll, then a random fallback if nobody activated. ported from
/// SillyTavern's `activateNaturalOrder` in `group-chats.js`.
pub fn activate_natural(
    candidates: &[ActivationCandidate],
    last_speaker: Option<&str>,
    trigger_text: &str,
    roll: &mut impl FnMut() -> f64,
) -> Vec<String> {
    let eligible: Vec<&ActivationCandidate> = candidates
        .iter()
        .filter(|c| Some(c.character_id.as_str()) != last_speaker)
        .collect();
    let eligible_name_words: Vec<Vec<String>> = eligible.iter().map(|c| extract_all_words(&c.name)).collect();

    let mut activated: Vec<String> = Vec::new();
    let mut activated_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for word in extract_all_words(trigger_text) {
        for (candidate, name_words) in eligible.iter().zip(&eligible_name_words) {
            if name_words.contains(&word) {
                if activated_set.insert(candidate.character_id.clone()) {
                    activated.push(candidate.character_id.clone());
                }
                break;
            }
        }
    }

    let mut chatty: Vec<&ActivationCandidate> = Vec::new();
    for candidate in &eligible {
        let r = roll();
        if candidate.talkativeness >= r && activated_set.insert(candidate.character_id.clone()) {
            activated.push(candidate.character_id.clone());
        }
        if candidate.talkativeness > 0.0 {
            chatty.push(candidate);
        }
    }

    if activated.is_empty() {
        // fallback pool is chatty members if any exist, else the full
        // original candidate list (not `eligible`), so yes that means the
        // banned last speaker can get picked here, matches real SillyTavern
        let pool: Vec<&ActivationCandidate> = if !chatty.is_empty() {
            chatty
        } else {
            candidates.iter().collect()
        };
        if !pool.is_empty() {
            let idx = pick_random_index(pool.len(), roll);
            activated.push(pool[idx].character_id.clone());
        }
    }

    activated
}

fn pick_random_index(len: usize, roll: &mut impl FnMut() -> f64) -> usize {
    let idx = (roll() * len as f64) as usize;
    idx.min(len.saturating_sub(1))
}

/// picks list or natural by `groups.activation_strategy`; unrecognized
/// strings fall back to list, matching the column's own `DEFAULT 'list'`.
pub fn resolve_activation(
    strategy: &str,
    candidates: &[ActivationCandidate],
    last_speaker: Option<&str>,
    trigger_text: &str,
    roll: &mut impl FnMut() -> f64,
) -> Vec<String> {
    match strategy {
        "natural" => activate_natural(candidates, last_speaker, trigger_text, roll),
        _ => activate_list(candidates),
    }
}

/// truncates a reply at the first line starting with another live member's
/// name + colon (SillyTavern's `cleanGroupMessage`). caller must already
/// exclude the speaker's own name from `other_member_names`.
pub fn clean_group_reply(reply: &str, other_member_names: &[String]) -> String {
    let mut cut_at: Option<usize> = None;
    for name in other_member_names {
        if name.is_empty() {
            continue;
        }
        let needle = format!("{name}:");
        let mut search_from = 0;
        while let Some(pos) = reply[search_from..].find(&needle) {
            let abs_pos = search_from + pos;
            let at_line_start = abs_pos == 0 || reply.as_bytes()[abs_pos - 1] == b'\n';
            if at_line_start {
                cut_at = Some(cut_at.map_or(abs_pos, |c| c.min(abs_pos)));
                break;
            }
            search_from = abs_pos + needle.len();
        }
    }
    match cut_at {
        Some(idx) => reply[..idx].to_string(),
        None => reply.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(character_id: &str, name: &str, talkativeness: f64) -> ActivationCandidate {
        ActivationCandidate {
            character_id: character_id.to_string(),
            name: name.to_string(),
            talkativeness,
        }
    }

    #[test]
    fn list_strategy_activates_every_candidate_in_the_order_given() {
        let candidates = vec![
            candidate("aria-id", "Aria", 0.5),
            candidate("beck-id", "Beck", 0.5),
            candidate("cass-id", "Cass", 0.5),
        ];

        let activated = activate_list(&candidates);

        assert_eq!(activated, vec!["aria-id", "beck-id", "cass-id"]);
    }

    #[test]
    fn list_strategy_on_an_empty_roster_activates_nobody() {
        assert_eq!(activate_list(&[]), Vec::<String>::new());
    }

    #[test]
    fn list_strategy_never_activates_a_disabled_member_because_the_caller_never_passes_one() {
        // this fn doesn't know what "disabled" even is, so filtering
        // happens on the caller's side
        let enabled_only = vec![candidate("aria-id", "Aria", 0.5)];
        assert_eq!(activate_list(&enabled_only), vec!["aria-id"]);
    }

    #[test]
    fn extract_all_words_lowercases_and_splits_on_word_boundaries() {
        assert_eq!(
            extract_all_words("Hello, world!"),
            vec!["hello".to_string(), "world".to_string()]
        );
    }

    #[test]
    fn extract_all_words_on_empty_text_returns_nothing() {
        assert_eq!(extract_all_words(""), Vec::<String>::new());
    }

    #[test]
    fn natural_activates_a_member_whose_name_is_mentioned_as_a_whole_word() {
        let candidates = vec![
            candidate("alice-id", "Alice", 0.0),
            candidate("bob-id", "Bob", 0.0),
        ];
        // roll pinned at 1.0 so talkativeness 0.0 never wins, isolates
        // mention detection from the roll phase
        let mut roll = || 1.0;

        let activated = activate_natural(&candidates, None, "hey alice, how are you", &mut roll);

        assert_eq!(activated, vec!["alice-id".to_string()]);
    }

    #[test]
    fn natural_mention_match_is_whole_word_not_substring() {
        let candidates = vec![
            candidate("al-id", "Al", 0.0),
            // bob always activates on the roll below so we don't fall
            // through to the fallback path and mask this assertion
            candidate("bob-id", "Bob", 1.0),
        ];
        let mut roll = || 0.5;

        let activated = activate_natural(&candidates, None, "Alice said hi to Bob", &mut roll);

        assert!(
            !activated.contains(&"al-id".to_string()),
            "\"Al\" must not match inside the word \"Alice\""
        );
        assert!(activated.contains(&"bob-id".to_string()));
    }

    #[test]
    fn natural_talkativeness_roll_activates_on_exact_equality_with_the_roll_value() {
        // real SillyTavern uses `talkativeness >= rollValue`, exact ties
        // activate. this is that >= boundary
        let candidates = vec![
            candidate("at-boundary-id", "AtBoundary", 0.5),
            candidate("below-boundary-id", "BelowBoundary", 0.499999),
        ];
        let mut roll = || 0.5;

        let activated = activate_natural(&candidates, None, "", &mut roll);

        assert_eq!(activated, vec!["at-boundary-id".to_string()]);
    }

    #[test]
    fn natural_bans_the_last_speaker_from_both_mention_and_talkativeness_phases() {
        let candidates = vec![
            candidate("alice-id", "Alice", 1.0),
            candidate("bob-id", "Bob", 1.0),
        ];
        // roll of 0.0 activates anyone, and alice gets mentioned by name
        // too, but she still can't show up, she's the banned last speaker
        let mut roll = || 0.0;

        let activated = activate_natural(&candidates, Some("alice-id"), "hey alice", &mut roll);

        assert!(!activated.contains(&"alice-id".to_string()));
        assert!(activated.contains(&"bob-id".to_string()));
    }

    #[test]
    fn natural_falls_back_to_a_random_member_when_nobody_activates() {
        let candidates = vec![
            candidate("aria-id", "Aria", 0.0),
            candidate("beck-id", "Beck", 0.0),
        ];
        // nobody's chatty and there's nothing to mention-match, so this
        // has to fall back to one random pick
        let mut roll = || 1.0;

        let activated = activate_natural(&candidates, None, "", &mut roll);

        assert_eq!(activated.len(), 1, "fallback must pick exactly one member, not zero or many");
        assert!(activated[0] == "aria-id" || activated[0] == "beck-id");
    }

    #[test]
    fn natural_fallback_can_reactivate_the_banned_last_speaker_when_nobody_else_is_chatty() {
        // real SillyTavern's fallback pool is chattyMembers if any exist,
        // else the raw unfiltered members param, so yeah, the banned last
        // speaker can get picked back up here. weird but that's upstream
        let candidates = vec![
            candidate("aria-id", "Aria", 0.0),
            candidate("beck-id", "Beck", 0.0),
        ];
        let mut roll = || 0.4;

        let activated = activate_natural(&candidates, Some("aria-id"), "", &mut roll);

        assert_eq!(activated, vec!["aria-id".to_string()]);
    }

    #[test]
    fn natural_on_an_empty_roster_activates_nobody() {
        let mut roll = || 0.5;
        assert_eq!(activate_natural(&[], None, "hello", &mut roll), Vec::<String>::new());
    }

    #[test]
    fn resolve_activation_dispatches_list_by_name() {
        let candidates = vec![candidate("a", "A", 0.5), candidate("b", "B", 0.5)];
        let mut roll = || 1.0;
        assert_eq!(
            resolve_activation("list", &candidates, None, "hi", &mut roll),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn resolve_activation_dispatches_natural_by_name() {
        let candidates = vec![candidate("a", "A", 0.0), candidate("b", "B", 1.0)];
        let mut roll = || 0.5;
        let activated = resolve_activation("natural", &candidates, None, "", &mut roll);
        assert!(activated.contains(&"b".to_string()));
    }

    #[test]
    fn resolve_activation_falls_back_to_list_for_an_unrecognized_strategy() {
        // matches the `activation_strategy` column's own DEFAULT 'list' in
        // 0024_groups.sql, an unrecognized value degrades to the same
        // deterministic behavior rather than panicking or silently doing
        // nothing.
        let candidates = vec![candidate("a", "A", 0.5)];
        let mut roll = || 1.0;
        assert_eq!(
            resolve_activation("something-unrecognized", &candidates, None, "hi", &mut roll),
            vec!["a".to_string()]
        );
    }

    #[test]
    fn clean_group_reply_truncates_at_another_members_name() {
        let others = vec!["Beck".to_string()];
        let reply = "Sure, let's go!\nBeck: wait for me";
        assert_eq!(clean_group_reply(reply, &others), "Sure, let's go!\n");
    }

    #[test]
    fn clean_group_reply_leaves_the_speakers_own_lines_alone() {
        // the current speaker's own name is never in `other_member_names`
        // (the caller excludes it), so a reply that happens to start with
        // the speaker's own "Name:" prefix (echoing the name-prefixed
        // history style back) must not be truncated against itself.
        let others = vec!["Beck".to_string()];
        let reply = "Aria: hi there, everyone";
        assert_eq!(clean_group_reply(reply, &others), "Aria: hi there, everyone");
    }

    #[test]
    fn clean_group_reply_with_no_match_returns_the_reply_unchanged() {
        let others = vec!["Beck".to_string(), "Cass".to_string()];
        assert_eq!(clean_group_reply("just a normal reply", &others), "just a normal reply");
    }

    #[test]
    fn clean_group_reply_truncates_at_the_earliest_of_several_matches() {
        let others = vec!["Beck".to_string(), "Cass".to_string()];
        let reply = "ok\nCass: no\nBeck: wait";
        assert_eq!(clean_group_reply(reply, &others), "ok\n");
    }
}
