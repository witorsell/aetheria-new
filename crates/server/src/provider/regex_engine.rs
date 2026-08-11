use crate::models::regex_script::RegexScript;
use crate::provider::prompt::substitute_macros;

// SillyTavern's regex_placement enum, just the two we actually use
pub const PLACEMENT_USER_INPUT: i32 = 1;
pub const PLACEMENT_AI_OUTPUT: i32 = 2;

// peels /pattern/flags off input like SillyTavern's regexFromString does.
// no leading / = whole string is the pattern, zero flags
fn parse_regex_source(input: &str) -> (String, String) {
    if let Some(rest) = input.strip_prefix('/') {
        if let Some(last_slash) = rest.rfind('/') {
            let pattern = rest[..last_slash].to_string();
            let after = &rest[last_slash + 1..];
            let flags: String = after.chars().take_while(|c| c.is_ascii_lowercase()).collect();
            let valid = "gmixXsuUAJ";
            let mut seen = std::collections::HashSet::new();
            let flags_ok = !flags.is_empty()
                && flags.chars().all(|c| valid.contains(c) && seen.insert(c));
            if flags.is_empty() || flags_ok {
                return (pattern, flags);
            }
        }
    }
    (input.to_string(), String::new())
}

// mirrors substituteRegex: 0 = leave macros alone, 1 = raw values, 2 = escaped
fn substitute_pattern_macros(pattern: &str, char_name: &str, user_name: &str, mode: i32) -> String {
    match mode {
        1 => substitute_macros(pattern, char_name, user_name),
        2 => substitute_macros(pattern, &regex::escape(char_name), &regex::escape(user_name)),
        _ => pattern.to_string(),
    }
}

// strips trim_strings entries out of a captured group, matches filterString
fn filter_string(raw: &str, trim_strings: &[String], char_name: &str, user_name: &str) -> String {
    let subs: Vec<String> = trim_strings
        .iter()
        .filter_map(|trim| {
            let sub = substitute_macros(trim, char_name, user_name);
            if sub.is_empty() { None } else { Some(sub) }
        })
        .collect();
    if subs.is_empty() {
        return raw.to_string();
    }
    let pattern = subs.iter()
        .map(|s| regex::escape(s))
        .collect::<Vec<_>>()
        .join("|");
    let re = regex::Regex::new(&pattern).expect("escaped patterns should compile");
    re.replace_all(raw, "").into_owned()
}

// expands $1/$<name> refs, matches runRegexScript's replaceWithGroups
fn build_replacement(
    caps: &regex::Captures,
    replace_string: &str,
    trim_strings: &[String],
    char_name: &str,
    user_name: &str,
) -> String {
    let mut out = String::new();
    let chars: Vec<char> = replace_string.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '<' {
                if let Some(end) = chars[i + 2..].iter().position(|&c| c == '>') {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    if let Some(m) = caps.name(&name) {
                        out.push_str(&filter_string(m.as_str(), trim_strings, char_name, user_name));
                    }
                    i = i + 2 + end + 1;
                    continue;
                }
            } else if chars[i + 1].is_ascii_digit() {
                let mut end = i + 1;
                while end < chars.len() && chars[end].is_ascii_digit() {
                    end += 1;
                }
                let num_str: String = chars[i + 1..end].iter().collect();
                if let Ok(num) = num_str.parse::<usize>() {
                    if let Some(m) = caps.get(num) {
                        out.push_str(&filter_string(m.as_str(), trim_strings, char_name, user_name));
                    }
                }
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    substitute_macros(&out, char_name, user_name)
}

/// applies one regex script to `text`, matching SillyTavern's `runRegexScript`.
pub fn apply_regex_script(text: &str, script: &RegexScript, char_name: &str, user_name: &str) -> String {
    let (pattern_raw, flags) = parse_regex_source(&script.find_regex);
    let pattern = substitute_pattern_macros(&pattern_raw, char_name, user_name, script.substitute_regex);

    let mut inline_flags = String::new();
    if flags.contains('i') {
        inline_flags.push('i');
    }
    if flags.contains('m') {
        inline_flags.push('m');
    }
    if flags.contains('s') {
        inline_flags.push('s');
    }
    let full_pattern = if inline_flags.is_empty() {
        pattern
    } else {
        format!("(?{inline_flags}){pattern}")
    };

    let Ok(re) = regex::Regex::new(&full_pattern) else {
        return text.to_string();
    };

    if flags.contains('g') {
        re.replace_all(text, |caps: &regex::Captures| {
            build_replacement(caps, &script.replace_string, &script.trim_strings, char_name, user_name)
        })
        .into_owned()
    } else {
        match re.captures(text) {
            Some(caps) => {
                let m = caps.get(0).expect("capture group 0 is always the whole match");
                let replacement = build_replacement(&caps, &script.replace_string, &script.trim_strings, char_name, user_name);
                format!("{}{}{}", &text[..m.start()], replacement, &text[m.end()..])
            }
            None => text.to_string(),
        }
    }
}

// runs text through every enabled prompt_only script whose
// placement/min_depth/max_depth cover it. depth 0 = newest message
pub fn apply_prompt_regex_scripts(
    scripts: &[RegexScript],
    text: &str,
    role: &str,
    depth: i32,
    char_name: &str,
    user_name: &str,
) -> String {
    let placement_code = if role == "user" { PLACEMENT_USER_INPUT } else { PLACEMENT_AI_OUTPUT };
    let mut result = text.to_string();
    for script in scripts {
        if script.disabled || !script.prompt_only {
            continue;
        }
        if !script.placement.contains(&placement_code) {
            continue;
        }
        if let Some(min) = script.min_depth {
            if min >= -1 && depth < min {
                continue;
            }
        }
        if let Some(max) = script.max_depth {
            if max >= 0 && depth > max {
                continue;
            }
        }
        result = apply_regex_script(&result, script, char_name, user_name);
    }
    result
}
