use server::models::character::Character;
use server::models::message::MessageNode;
use server::models::preset::{Preset, PresetOrderEntry, PresetPrompt};
use server::models::regex_script::RegexScript;
use server::provider::prompt::{build_messages, PromptContext};
use server::provider::regex_engine::apply_regex_script;

fn test_character() -> Character {
    Character {
        user_id: 1,
        id: "char-1".to_string(),
        name: "Seraphina".to_string(),
        description: "A forest guardian.".to_string(),
        personality: "Warm, curious.".to_string(),
        scenario: "A quiet glade.".to_string(),
        first_message: "Hello there, traveler.".to_string(),
        avatar_path: None,
        avatar_url: None,
        sample_chat: "Example banter.".to_string(),
        system_prompt: String::new(),
        post_history_instructions: String::new(),
        prefill: String::new(),
        insert_depth_prompt: String::new(),
        insert_depth: 0,
        talkativeness: 0.5,
        persona: "{}".to_string(),
        extensions: "{}".to_string(),
        folder_id: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn make_node(role: &str, content: &str) -> MessageNode {
    MessageNode {
        user_id: 1,
        id: uuid::Uuid::new_v4().to_string(),
        parent_id: None,
        role: role.to_string(),
        content: content.to_string(),
        visible: true,
        deleted: false,
        created_at: 0,
        children: vec![],
        raw_prompt: None,
        prompt_tokens: None,
        context_limit: None,
        character_id: None,
    }
}

fn strip_html_script() -> RegexScript {
    RegexScript {
        id: "7f48cf2c".to_string(),
        user_id: 1,
        script_name: "Strip HTML/GFX from Context".to_string(),
        find_regex: r"<\/?(?:div|span|strong|font|b|i|u|br|p|h1|h2|h3)(?:\s[^>]*)?>|<!-- GFX_START -->|<!-- GFX_END -->|&nbsp;".to_string(),
        replace_string: " ".to_string(),
        trim_strings: vec![],
        placement: vec![1, 2],
        disabled: false,
        markdown_only: false,
        prompt_only: true,
        run_on_edit: false,
        substitute_regex: 0,
        min_depth: None,
        max_depth: None,
        created_at: 0,
    }
}

fn strip_plot_momentum_script() -> RegexScript {
    RegexScript {
        id: "7c0cf9b2".to_string(),
        user_id: 1,
        script_name: "Strip Old Plot Momentum".to_string(),
        find_regex: r"<details>\s*<summary>Plot Momentum<\/summary>[\s\S]*?<\/details>".to_string(),
        replace_string: " ".to_string(),
        trim_strings: vec![],
        placement: vec![2, 1],
        disabled: false,
        markdown_only: false,
        prompt_only: true,
        run_on_edit: false,
        substitute_regex: 0,
        min_depth: Some(2),
        max_depth: None,
        created_at: 0,
    }
}

#[test]
fn undelimited_find_regex_replaces_only_the_first_match_like_sillytavern_does() {
    // no `/pattern/flags` delimiters in this findRegex, so SillyTavern's own
    // regexFromString parses it with no flags at all, not even implicit
    // global. only the first `<div>` gets stripped, `</div>` and `<b>` do not.
    let script = strip_html_script();
    let out = apply_regex_script("Hello <div>World</div> <b>bold</b> &nbsp; end", &script, "Char", "User");
    // "<div>" (the first match) is replaced by the script's own replaceString
    // (a single space), landing next to the space already before it.
    assert_eq!(out, "Hello  World</div> <b>bold</b> &nbsp; end");
}

#[test]
fn strips_gfx_markers_too() {
    let script = strip_html_script();
    let out = apply_regex_script("before <!-- GFX_START --> stuff", &script, "Char", "User");
    assert_eq!(out, "before   stuff");
}

#[test]
fn plot_momentum_regex_matches_the_whole_details_block() {
    let script = strip_plot_momentum_script();
    let text = "Visible reply.\n<details>\n<summary>Plot Momentum</summary>\n- NPC_Agenda: foo\n</details>";
    let out = apply_regex_script(text, &script, "Char", "User");
    assert_eq!(out, "Visible reply.\n ");
}

#[test]
fn prompt_only_regex_scripts_are_applied_to_history_by_role_placement_and_depth() {
    // depth convention: the new user message is depth 0, the newest history
    // message is depth 1, etc. minDepth: 2 on the plot-momentum script means
    // the newest assistant reply (depth 1) keeps its block, only older ones
    // (depth >= 2) get stripped.
    let history = vec![
        make_node("assistant", "Older reply.\n<details>\n<summary>Plot Momentum</summary>\nstuff\n</details>"),
        make_node("user", "ok continue"),
        make_node("assistant", "Newest reply.\n<details>\n<summary>Plot Momentum</summary>\nstuff\n</details>"),
    ];
    let scripts = vec![strip_plot_momentum_script()];

    let messages = build_messages(PromptContext {
        character: &test_character(),
        history: &history,
        new_user_message: "go on",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "User",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &scripts,
        active_preset: None,
        respond_as_user: false,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    });

    let older_reply = messages.iter().find(|m| m.content.starts_with("Older reply.")).unwrap();
    assert!(!older_reply.content.contains("Plot Momentum"), "older assistant turn should be stripped: {older_reply:?}");

    let newest_reply = messages.iter().find(|m| m.content.starts_with("Newest reply.")).unwrap();
    assert!(newest_reply.content.contains("Plot Momentum"), "newest assistant turn (depth 1) should be kept intact: {newest_reply:?}");
}

fn small_preset() -> Preset {
    Preset {
        id: "preset-1".to_string(),
        user_id: 1,
        name: "Test Preset".to_string(),
        prompts: vec![
            PresetPrompt { identifier: "main".to_string(), name: "Main".to_string(), content: "You are the narrator.".to_string(), role: "system".to_string(), marker: false, injection_position: 0, injection_depth: 4 },
            PresetPrompt { identifier: "charDescription".to_string(), name: "Char Description".to_string(), content: String::new(), role: "system".to_string(), marker: true, injection_position: 0, injection_depth: 4 },
            PresetPrompt { identifier: "chatHistory".to_string(), name: "Chat History".to_string(), content: String::new(), role: "system".to_string(), marker: true, injection_position: 0, injection_depth: 4 },
            PresetPrompt { identifier: "afterHistory".to_string(), name: "After History".to_string(), content: "Stay concise.".to_string(), role: "user".to_string(), marker: false, injection_position: 0, injection_depth: 4 },
            PresetPrompt { identifier: "disabledOne".to_string(), name: "Disabled".to_string(), content: "should never appear".to_string(), role: "system".to_string(), marker: false, injection_position: 0, injection_depth: 4 },
        ],
        prompt_order: vec![
            PresetOrderEntry { identifier: "main".to_string(), enabled: true },
            PresetOrderEntry { identifier: "charDescription".to_string(), enabled: true },
            PresetOrderEntry { identifier: "chatHistory".to_string(), enabled: true },
            PresetOrderEntry { identifier: "afterHistory".to_string(), enabled: true },
            PresetOrderEntry { identifier: "disabledOne".to_string(), enabled: false },
        ],
        created_at: 0,
        updated_at: 0,
    }
}

#[test]
fn preset_mode_orders_messages_by_prompt_order_and_splices_history_at_the_chathistory_marker() {
    let preset = small_preset();
    let history = vec![make_node("user", "earlier turn")];

    let messages = build_messages(PromptContext {
        character: &test_character(),
        history: &history,
        new_user_message: "the real question",
        system_prompt_suffix: "ignored in preset mode",
        post_history_instructions: "also ignored in preset mode",
        context_limit: 0,
        user_name: "User",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: Some(&preset),
        respond_as_user: false,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    });

    let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(
        contents,
        vec![
            "You are the narrator.",
            "A forest guardian.",
            "earlier turn",
            "the real question",
            "Stay concise.",
        ]
    );
    assert!(!contents.contains(&"should never appear"), "disabled prompt_order entries must not be emitted");

    // the trailing "user"-role instruction after chatHistory in prompt_order
    // really does land after the user's real message, not before it.
    let question_idx = contents.iter().position(|c| *c == "the real question").unwrap();
    let stay_concise_idx = contents.iter().position(|c| *c == "Stay concise.").unwrap();
    assert!(stay_concise_idx > question_idx);
}

#[test]
fn preset_mode_falls_back_to_appending_history_when_chathistory_marker_is_missing() {
    let mut preset = small_preset();
    preset.prompt_order.retain(|e| e.identifier != "chatHistory");

    let messages = build_messages(PromptContext {
        character: &test_character(),
        history: &[],
        new_user_message: "hello",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "User",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: Some(&preset),
        respond_as_user: false,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    });

    assert!(messages.iter().any(|m| m.content == "hello"), "the new user message must still reach the model: {messages:?}");
}
