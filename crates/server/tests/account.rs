mod common;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use futures_util::StreamExt;
use std::future::IntoFuture;
use tower::ServiceExt;

#[tokio::test]
async fn export_all_includes_seeded_content_and_resolves_cross_references() {
    let (app, cookie) = common::authed_app().await;

    // character with a folder, a tag, and an alternate greeting
    let folder = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/folders")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"My Folder"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(folder.status(), StatusCode::OK);
    let folder_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(folder.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let folder_id = folder_json["id"].as_str().unwrap().to_string();

    let character = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(
                r#"{{"name":"Aeth","first_message":"hello","folder_id":"{folder_id}"}}"#
            ))).unwrap(),
    ).await.unwrap();
    assert_eq!(character.status(), StatusCode::OK);
    let character_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(character.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let character_id = character_json["id"].as_str().unwrap().to_string();

    // a chat with one exchange
    let chat = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/characters/{character_id}/chats"))
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"Test Chat"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(chat.status(), StatusCode::OK);

    let response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let export: server::models::account::AccountExport = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(export.characters.len(), 1);
    assert_eq!(export.characters[0].name, "Aeth");
    assert_eq!(export.characters[0].folder_name.as_deref(), Some("My Folder"));
    assert_eq!(export.chats.len(), 1);
    assert_eq!(export.chats[0].character_id.as_deref(), Some(character_id.as_str()));
    // the greeting gets persisted as real history the instant the chat is
    // created (see routes/chats.rs create), so it shows up as message history
    assert_eq!(export.chats[0].messages.len(), 1);
    assert_eq!(export.chats[0].messages[0].content, "hello");
    assert_eq!(export.chats[0].messages[0].parent_id, None);
}

/// a minimal mock OpenAI-compatible server, just enough to let `generate`
/// and `regenerate` produce a real assistant reply without hitting a real
/// provider. modeled on the same helper in generate_streaming.rs and
/// group_generation.rs, trimmed down since this test only needs branching
/// messages to exist, not to inspect their content or timing.
async fn spawn_mock_provider() -> String {
    async fn mock_completions() -> axum::response::sse::Sse<impl futures_util::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
        let chunks = vec![
            "{\"choices\":[{\"delta\":{\"content\":\"A reply.\"}}]}".to_string(),
            "[DONE]".to_string(),
        ];
        axum::response::sse::Sse::new(futures_util::stream::iter(chunks).map(|c| Ok(axum::response::sse::Event::default().data(c))))
    }
    let app = axum::Router::new().route("/chat/completions", axum::routing::post(mock_completions));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    format!("http://{addr}")
}

#[tokio::test]
async fn nuclear_delete_wipes_content_for_that_user_only() {
    let (app, cookie_a, cookie_b) = common::authed_app_with_second_user().await;

    // point A's settings at a mock provider so branching messages (via
    // generate + regenerate) can be created without a real LLM
    let mock_base_url = spawn_mock_provider().await;
    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    }).to_string();
    let set_settings = app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/settings")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(settings_body)).unwrap(),
    ).await.unwrap();
    assert_eq!(set_settings.status(), StatusCode::OK);

    // capture what /api/settings reports before touching any content, to
    // compare against after delete - this is the feature's "your API
    // keys/settings are untouched" promise, which had no test coverage
    let settings_before_resp = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/settings")
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let settings_before_bytes = axum::body::to_bytes(settings_before_resp.into_body(), usize::MAX).await.unwrap();
    let settings_before: serde_json::Value = serde_json::from_slice(&settings_before_bytes).unwrap();

    // character
    let char_a = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"A's Character"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(char_a.status(), StatusCode::OK);
    let char_a_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(char_a.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let char_a_id = char_a_json["id"].as_str().unwrap().to_string();

    let char_b = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_b.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"B's Character"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(char_b.status(), StatusCode::OK);

    // lorebook with an entry
    let lorebook = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/lorebooks")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"A's Lorebook"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(lorebook.status(), StatusCode::OK);
    let lorebook_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(lorebook.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let lorebook_id = lorebook_json["id"].as_str().unwrap().to_string();

    let entry = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/lorebooks/{lorebook_id}/entries"))
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"{{"lorebook_id":"{lorebook_id}","name":"Entry","entry":"Some lore"}}"#))).unwrap(),
    ).await.unwrap();
    assert_eq!(entry.status(), StatusCode::OK);

    // group
    let group = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/groups")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"A's Group"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(group.status(), StatusCode::OK);

    // preset (import endpoint, multipart)
    let (content_type, body) = common::multipart_body("file", "MyPreset.json", "application/json", br#"{"prompts":[]}"#);
    let preset = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/presets")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap();
    assert_eq!(preset.status(), StatusCode::OK);

    // regex script (also an import endpoint, multipart)
    let (content_type, body) = common::multipart_body(
        "file", "script.json", "application/json",
        br#"{"scriptName":"A Script","findRegex":"foo"}"#,
    );
    let regex_script = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/regex-scripts")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap();
    assert_eq!(regex_script.status(), StatusCode::OK);

    // theme
    let theme = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/themes")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"A's Theme","tokens":{}}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(theme.status(), StatusCode::OK);

    // chat with branching messages: create the chat (gives a root greeting),
    // generate a user message + assistant reply, then regenerate a second,
    // sibling assistant reply off that same user message - two children
    // under one parent is a genuine branch, not just a linear thread
    let chat = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/characters/{char_a_id}/chats"))
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"A's Chat"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(chat.status(), StatusCode::OK);
    let chat_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(chat.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    let generate_resp = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"content":"Hi!"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(generate_resp.status(), StatusCode::OK);
    // drain the SSE stream so the write it triggers actually completes
    // before the test moves on
    let mut body = generate_resp.into_body().into_data_stream();
    while body.next().await.is_some() {}

    let tree_resp = app.clone().oneshot(
        Request::builder().method("GET").uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let tree_bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&tree_bytes).unwrap();
    let user_msg_id = tree["messages"].as_object().unwrap().values()
        .find(|m| m["role"] == "user")
        .expect("the generate call above should have created a user message")
        ["id"].as_str().unwrap().to_string();

    let regenerate_resp = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/regenerate?parent_id={user_msg_id}"))
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(regenerate_resp.status(), StatusCode::OK);
    let mut body = regenerate_resp.into_body().into_data_stream();
    while body.next().await.is_some() {}

    let delete_response = app.clone().oneshot(
        Request::builder().method("DELETE").uri("/api/account/data")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"username":"testuser"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let a_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let a_bytes = axum::body::to_bytes(a_export.into_body(), usize::MAX).await.unwrap();
    let a: server::models::account::AccountExport = serde_json::from_slice(&a_bytes).unwrap();
    assert_eq!(a.characters.len(), 0);
    assert_eq!(a.lorebooks.len(), 0);
    assert_eq!(a.groups.len(), 0);
    assert_eq!(a.chats.len(), 0);
    assert_eq!(a.presets.len(), 0);
    assert_eq!(a.regex_scripts.len(), 0);
    assert_eq!(a.themes.len(), 0);

    let b_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_b.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let b_bytes = axum::body::to_bytes(b_export.into_body(), usize::MAX).await.unwrap();
    let b: server::models::account::AccountExport = serde_json::from_slice(&b_bytes).unwrap();
    assert_eq!(b.characters.len(), 1);
    assert_eq!(b.characters[0].name, "B's Character");

    // the central promise of the feature: content deletion, not account
    // deletion - settings (api keys, provider config, sampling, etc.)
    // must come back byte-for-byte identical
    let settings_after_resp = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/settings")
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let settings_after_bytes = axum::body::to_bytes(settings_after_resp.into_body(), usize::MAX).await.unwrap();
    let settings_after: serde_json::Value = serde_json::from_slice(&settings_after_bytes).unwrap();
    assert_eq!(settings_before, settings_after, "settings must survive nuclear delete untouched");
}

#[tokio::test]
async fn nuclear_delete_requires_correct_username() {
    let (app, cookie) = common::authed_app().await;

    let response = app.clone().oneshot(
        Request::builder().method("DELETE").uri("/api/account/data")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"username":"wrong-name"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn export_then_import_round_trips_into_a_fresh_account() {
    let (app, cookie_a, cookie_b) = common::authed_app_with_second_user().await;

    let character = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Aeth","first_message":"hi there"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(character.status(), StatusCode::OK);
    let character_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(character.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let character_id = character_json["id"].as_str().unwrap().to_string();

    // a chat's greeting only becomes real message history once a chat is
    // created for the character (see routes/chats.rs create) - creating
    // the character alone doesn't produce a chat to export
    let chat = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/characters/{character_id}/chats"))
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"Test Chat"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(chat.status(), StatusCode::OK);

    let export_response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let export_bytes = axum::body::to_bytes(export_response.into_body(), usize::MAX).await.unwrap();

    let import_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/account/import-all")
            .header(header::COOKIE, cookie_b.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(export_bytes)).unwrap(),
    ).await.unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);

    let b_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_b.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let b_bytes = axum::body::to_bytes(b_export.into_body(), usize::MAX).await.unwrap();
    let b: server::models::account::AccountExport = serde_json::from_slice(&b_bytes).unwrap();

    assert_eq!(b.characters.len(), 1);
    assert_eq!(b.characters[0].name, "Aeth");
    assert_ne!(b.characters[0].id, ""); // got a real fresh id
    assert_eq!(b.chats.len(), 1);
    assert_eq!(b.chats[0].character_id.as_deref(), Some(b.characters[0].id.as_str()));
    assert_eq!(b.chats[0].messages.len(), 1);
    assert_eq!(b.chats[0].messages[0].content, "hi there");
}

#[tokio::test]
async fn import_is_additive_not_destructive() {
    let (app, cookie) = common::authed_app().await;

    let first = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Already Here"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let export_response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let export_bytes = axum::body::to_bytes(export_response.into_body(), usize::MAX).await.unwrap();
    let mut export: server::models::account::AccountExport = serde_json::from_slice(&export_bytes).unwrap();
    export.characters[0].name = "Imported Copy".to_string();
    let reimport_body = serde_json::to_vec(&export).unwrap();

    let import_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/account/import-all")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(reimport_body)).unwrap(),
    ).await.unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);

    let final_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let final_bytes = axum::body::to_bytes(final_export.into_body(), usize::MAX).await.unwrap();
    let final_state: server::models::account::AccountExport = serde_json::from_slice(&final_bytes).unwrap();

    let names: Vec<&str> = final_state.characters.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Already Here"));
    assert!(names.contains(&"Imported Copy"));
    assert_eq!(final_state.characters.len(), 2);
}

#[tokio::test]
async fn import_survives_a_tag_name_that_already_exists_on_another_account() {
    // tags.name carries a bare, instance-wide UNIQUE constraint (not scoped
    // by user_id - see migrations/0001_init.sql). account A creates a tag
    // named "Fantasy" and exports a character carrying it; account B, which
    // has never touched tags itself, then imports that export. the "Fantasy"
    // row already exists in the table (owned by A) by the time B's import
    // runs, so a plain INSERT would hit the UNIQUE constraint and roll back
    // the whole transaction - character, chat, everything - even though B's
    // own account had nothing conflicting. the fix must let this succeed.
    //
    // (note: the reused tag ends up "owned" by whichever account originally
    // created that name - here, A - since the schema has no per-user
    // scoping to fall back to. that's a pre-existing consequence of the
    // bare UNIQUE constraint, not something in scope to fix here, so this
    // test doesn't assert anything about tag visibility from B's own
    // perspective - only that the import itself is no longer all-or-nothing
    // on a tag-name collision.)
    let (app, cookie_a, cookie_b) = common::authed_app_with_second_user().await;

    let tag = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/tags")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Fantasy"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(tag.status(), StatusCode::OK);
    let tag_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(tag.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let tag_id = tag_json["id"].as_str().unwrap().to_string();

    let character = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Aeth"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(character.status(), StatusCode::OK);
    let character_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(character.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let character_id = character_json["id"].as_str().unwrap().to_string();

    let set_tags = app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/characters/{character_id}/tags"))
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"["{tag_id}"]"#))).unwrap(),
    ).await.unwrap();
    assert_eq!(set_tags.status(), StatusCode::OK);

    let export_response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_a.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let export_bytes = axum::body::to_bytes(export_response.into_body(), usize::MAX).await.unwrap();

    let import_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/account/import-all")
            .header(header::COOKIE, cookie_b.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(export_bytes)).unwrap(),
    ).await.unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);

    let b_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_b.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let b_bytes = axum::body::to_bytes(b_export.into_body(), usize::MAX).await.unwrap();
    let b: server::models::account::AccountExport = serde_json::from_slice(&b_bytes).unwrap();

    // the whole import landed - the tag-name collision didn't roll back
    // the character (or anything else) that came with it
    assert_eq!(b.characters.len(), 1);
    assert_eq!(b.characters[0].name, "Aeth");
}

#[tokio::test]
async fn delete_then_reimport_restores_character_lorebooks_and_chat_customization() {
    // the feature's actual headline flow - export, nuclear delete, restore
    // from that export - and specifically the two attachments that are easy
    // to lose along the way: a character's lorebook(s), and whether a
    // chat's lorebook selection was ever customized away from the
    // character's own set. both live in join tables (`character_lorebooks`,
    // `chats.lorebooks_customized`) that a naive export/import can silently
    // drop even though the content tables all round-trip fine.
    let (app, cookie) = common::authed_app().await;

    let lorebook = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/lorebooks")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Ancient Lore"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(lorebook.status(), StatusCode::OK);
    let lorebook_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(lorebook.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let lorebook_id = lorebook_json["id"].as_str().unwrap().to_string();

    let character = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"Aeth"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(character.status(), StatusCode::OK);
    let character_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(character.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let character_id = character_json["id"].as_str().unwrap().to_string();

    let attach_char_lorebook = app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/characters/{character_id}/lorebooks"))
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"{{"lorebook_ids":["{lorebook_id}"]}}"#))).unwrap(),
    ).await.unwrap();
    assert_eq!(attach_char_lorebook.status(), StatusCode::OK);

    let chat = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/characters/{character_id}/chats"))
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"Test Chat"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(chat.status(), StatusCode::OK);
    let chat_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(chat.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    // customizing the chat's lorebooks is what flips `lorebooks_customized`
    // to 1 - without that flag surviving the round-trip, this chat's
    // chat_lorebooks row would be reimported but never actually consulted
    // (generation_orchestrator and the lorebooks route both fall back to
    // the character's own lorebooks whenever the flag reads false)
    let attach_chat_lorebook = app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/chats/{chat_id}/lorebooks"))
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(format!(r#"{{"lorebook_ids":["{lorebook_id}"]}}"#))).unwrap(),
    ).await.unwrap();
    assert_eq!(attach_chat_lorebook.status(), StatusCode::OK);

    let export_response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let export_bytes = axum::body::to_bytes(export_response.into_body(), usize::MAX).await.unwrap();
    let export: server::models::account::AccountExport = serde_json::from_slice(&export_bytes).unwrap();

    // sanity check the export itself actually carries what was just set up,
    // before deleting anything
    assert_eq!(export.characters[0].lorebook_ids, vec![lorebook_id.clone()]);
    assert!(export.chats[0].lorebooks_customized);
    assert_eq!(export.chats[0].lorebook_ids, vec![lorebook_id.clone()]);

    let delete_response = app.clone().oneshot(
        Request::builder().method("DELETE").uri("/api/account/data")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"username":"testuser"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(delete_response.status(), StatusCode::OK);

    let import_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/account/import-all")
            .header(header::COOKIE, cookie.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(export_bytes)).unwrap(),
    ).await.unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);

    let restored_response = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let restored_bytes = axum::body::to_bytes(restored_response.into_body(), usize::MAX).await.unwrap();
    let restored: server::models::account::AccountExport = serde_json::from_slice(&restored_bytes).unwrap();

    assert_eq!(restored.characters.len(), 1);
    assert_eq!(restored.lorebooks.len(), 1);
    let new_lorebook_id = &restored.lorebooks[0].id;
    assert_ne!(new_lorebook_id, &lorebook_id, "reimport always mints fresh ids");

    // the character's lorebook attachment survived the round trip, remapped
    // to the freshly-imported lorebook's new id
    assert_eq!(restored.characters[0].lorebook_ids, vec![new_lorebook_id.clone()]);

    // the chat's customization flag and its own lorebook attachment both
    // survived too, remapped the same way
    assert_eq!(restored.chats.len(), 1);
    assert!(restored.chats[0].lorebooks_customized, "lorebooks_customized must survive import, or chat_lorebooks rows are dead weight");
    assert_eq!(restored.chats[0].lorebook_ids, vec![new_lorebook_id.clone()]);
}
