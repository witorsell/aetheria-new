use server::models::character::Character;
use server::models::message::MessageNode;
use server::provider::prompt::{build_messages, ChatMessage, PromptContext, Role};

#[allow(clippy::too_many_arguments)]
fn assemble(
    character: &Character,
    history: &[MessageNode],
    new_user_message: &str,
    system_prompt_suffix: &str,
    post_history_instructions: &str,
    context_limit: usize,
    user_name: &str,
    user_persona: Option<&str>,
) -> Vec<ChatMessage> {
    build_messages(PromptContext {
        character,
        history,
        new_user_message,
        system_prompt_suffix,
        post_history_instructions,
        context_limit,
        user_name,
        user_persona,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: None,
        respond_as_user: false,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    })
}

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
        sample_chat: String::new(),
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

#[test]
fn assembles_system_prompt_from_character_fields_and_global_suffix() {
    let messages = assemble(&test_character(), &[], "Hi!", "Stay in character.", "", 0, "User", None);

    assert_eq!(messages[0].role, Role::System);
    assert!(messages[0].content.contains("A forest guardian."));
    assert!(messages[0].content.contains("Warm, curious."));
    assert!(messages[0].content.contains("A quiet glade."));
    assert!(messages[0].content.contains("Stay in character."));
}

#[test]
fn includes_first_message_as_opening_assistant_turn_when_history_is_empty() {
    let messages = assemble(&test_character(), &[], "Hi!", "", "", 0, "User", None);

    assert_eq!(messages[1].role, Role::Assistant);
    assert_eq!(messages[1].content, "Hello there, traveler.");
    assert_eq!(messages[2].role, Role::User);
    assert_eq!(messages[2].content, "Hi!");
}

#[test]
fn skips_first_message_when_history_already_exists() {
    let history = vec![make_node("user", "earlier turn")];
    let messages = assemble(&test_character(), &history, "Hi!", "", "", 0, "User", None);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].content, "earlier turn");
    assert_eq!(messages[2].content, "Hi!");
}

#[test]
fn strips_stored_reasoning_from_assistant_history_before_replaying_it() {
    let history = vec![make_node(
        "assistant",
        "<think>internal monologue</think>The visible reply.",
    )];
    let messages = assemble(&test_character(), &history, "Hi!", "", "", 0, "User", None);

    assert_eq!(messages[1].content, "The visible reply.");
    assert!(!messages[1].content.contains("internal monologue"));
}

#[test]
fn drops_oldest_history_first_when_over_the_context_limit() {
    let history = vec![
        make_node("user", "first turn, very old"),
        make_node("assistant", "first reply, very old"),
        make_node("user", "second turn, more recent"),
        make_node("assistant", "second reply, more recent"),
    ];

    // a limit small enough that only the newest history pair plus the
    // fixed system/user messages can possibly fit.
    let messages = assemble(&test_character(), &history, "Hi!", "", "", 40, "User", None);

    assert!(
        !messages.iter().any(|m| m.content.contains("very old")),
        "oldest history should be dropped first: {messages:?}"
    );
    assert!(messages.iter().any(|m| m.content.contains("more recent")));
    assert_eq!(messages.last().unwrap().content, "Hi!");
}

#[test]
fn zero_context_limit_means_unlimited() {
    let history: Vec<_> = (0..50)
        .map(|i| make_node("user", &format!("turn number {i}")))
        .collect();
    let messages = assemble(&test_character(), &history, "Hi!", "", "", 0, "User", None);

    // system message + 50 history + new user message
    assert_eq!(messages.len(), 52);
}

#[test]
fn post_history_instructions_are_injected_right_before_the_new_user_message() {
    let history = vec![make_node("user", "earlier turn")];
    let messages = assemble(
        &test_character(),
        &history,
        "Hi!",
        "",
        "Stay in character no matter what.",
        0,
        "User",
        None,
    );

    assert_eq!(messages.len(), 4);
    let last_two: Vec<_> = messages.iter().rev().take(2).collect();
    assert_eq!(last_two[1].role, Role::System);
    assert_eq!(last_two[1].content, "Stay in character no matter what.");
    assert_eq!(last_two[0].role, Role::User);
    assert_eq!(last_two[0].content, "Hi!");
}

#[test]
fn post_history_instructions_are_never_dropped_by_the_context_limit() {
    let history = vec![
        make_node("user", "first turn, very old"),
        make_node("assistant", "first reply, very old"),
    ];
    let messages = assemble(
        &test_character(),
        &history,
        "Hi!",
        "",
        "Always stay in character.",
        20,
        "User",
        None,
    );

    assert!(
        messages.iter().any(|m| m.content == "Always stay in character."),
        "post-history instructions should survive even a very tight limit: {messages:?}"
    );
    assert_eq!(messages.last().unwrap().content, "Hi!");
}

#[test]
fn respond_as_user_swaps_history_roles_and_appends_no_new_user_turn() {
    let history = vec![
        make_node("user", "Hey there."),
        make_node("assistant", "Well met, traveler."),
    ];
    let messages = build_messages(PromptContext {
        character: &test_character(),
        history: &history,
        new_user_message: "",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "Testuser",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: None,
        respond_as_user: true,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    });

    let swapped_user_turn = messages.iter().find(|m| m.content == "Hey there.").unwrap();
    assert_eq!(swapped_user_turn.role, Role::Assistant, "the human's own past turn becomes the assistant example: {messages:?}");

    let swapped_char_turn = messages.iter().find(|m| m.content == "Well met, traveler.").unwrap();
    assert_eq!(swapped_char_turn.role, Role::User, "the character's past turn becomes user input: {messages:?}");

    assert!(!messages.iter().any(|m| m.content.trim().is_empty()), "no empty placeholder turn should be appended: {messages:?}");
    assert!(
        messages.last().unwrap().content.contains("Testuser"),
        "a trailing instruction naming the human should be appended last: {messages:?}"
    );
}

#[test]
fn continuation_sends_the_unfinished_reply_as_an_assistant_turn_not_a_user_turn() {
    let history = vec![make_node("user", "Tell me a story.")];
    let messages = build_messages(PromptContext {
        character: &test_character(),
        history: &history,
        new_user_message: "Once upon a time, the forest guardian",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "Testuser",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: None,
        respond_as_user: false,
        continuation: true,
        speaker_names: None,
        group_nudge: None,
    });

    let last = messages.last().unwrap();
    assert_eq!(last.role, Role::Assistant, "the unfinished reply must be sent as the model's own trailing turn, not put in the human's mouth: {messages:?}");
    assert_eq!(last.content, "Once upon a time, the forest guardian");
}

#[test]
fn continuation_skips_character_prefill_since_the_unfinished_reply_is_already_the_seed() {
    let mut character = test_character();
    character.prefill = "As the forest guardian, I say:".to_string();
    let messages = build_messages(PromptContext {
        character: &character,
        history: &[],
        new_user_message: "Once upon a time",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "Testuser",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: None,
        respond_as_user: false,
        continuation: true,
        speaker_names: None,
        group_nudge: None,
    });

    assert!(
        !messages.iter().any(|m| m.content.contains("I say:")),
        "prefill would stack a second assistant message after the continuation seed: {messages:?}"
    );
    assert_eq!(messages.last().unwrap().content, "Once upon a time");
}

#[test]
fn respond_as_user_skips_character_prefill() {
    let mut character = test_character();
    character.prefill = "As the forest guardian, I say:".to_string();
    let messages = build_messages(PromptContext {
        character: &character,
        history: &[],
        new_user_message: "",
        system_prompt_suffix: "",
        post_history_instructions: "",
        context_limit: 0,
        user_name: "Testuser",
        user_persona: None,
        lorebook_before: "",
        lorebook_after: "",
        regex_scripts: &[],
        active_preset: None,
        respond_as_user: true,
        continuation: false,
        speaker_names: None,
        group_nudge: None,
    });

    assert!(
        !messages.iter().any(|m| m.content.contains("I say:")),
        "the character's own prefill has no business seeding a message written in the human's voice: {messages:?}"
    );
}
