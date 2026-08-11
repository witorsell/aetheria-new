mod common;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::routing::post;
use futures_util::{stream, StreamExt};
use http_body_util::BodyExt;
use std::future::IntoFuture;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower::ServiceExt;

/// same shape as generate_streaming.rs's mock, but the reply text is
/// distinct per invocation so per-member replies can be told apart. the
/// real `OpenAIProvider` client never sends a distinguishing header on its
/// requests, so call order is tracked server-side with a per-test counter
/// instead (an earlier version of this mock kept an `x-mock-call` header
/// nothing ever set, so every call silently returned the same "reply-0").
async fn spawn_mock_provider() -> String {
    async fn mock_completions(
        State(call_count): State<Arc<AtomicUsize>>,
        _body: Body,
    ) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
        let call_n = call_count.fetch_add(1, Ordering::SeqCst);
        let chunks = vec![
            format!("{{\"choices\":[{{\"delta\":{{\"content\":\"reply-{call_n}\"}}}}]}}"),
            "[DONE]".to_string(),
        ];
        let stream = stream::iter(chunks).map(|c| Ok(Event::default().data(c)));
        Sse::new(stream)
    }
    let call_count = Arc::new(AtomicUsize::new(0));
    let app = axum::Router::new().route("/chat/completions", post(mock_completions)).with_state(call_count);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    format!("http://{addr}")
}

async fn create_character(app: &axum::Router, cookie: &str, name: &str) -> String {
    let body = serde_json::json!({ "name": name }).to_string();
    let response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string()
}

async fn drain(response: axum::http::Response<Body>) -> String {
    let mut body = response.into_body();
    let mut collected = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Some(data) = frame.unwrap().data_ref() {
            collected.extend_from_slice(data);
        }
    }
    String::from_utf8(collected).unwrap()
}

#[tokio::test]
async fn sending_a_message_to_a_list_group_gets_every_member_to_reply_in_order() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app().await;

    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    }).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/settings")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(settings_body)).unwrap(),
    ).await.unwrap();

    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;

    let group_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let group_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/groups")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(group_body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(group_response.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let members_body = serde_json::json!({ "members": [
        { "character_id": aria_id, "disabled": false },
        { "character_id": beck_id, "disabled": false },
    ]}).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/groups/{group_id}/members"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(members_body)).unwrap(),
    ).await.unwrap();

    let chat_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/groups/{group_id}/chats"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"title": "Chat"}).to_string())).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let generate_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"content": "hi both of you"}).to_string())).unwrap(),
    ).await.unwrap();
    assert_eq!(generate_response.status(), StatusCode::OK);
    let sse_text = drain(generate_response).await;
    assert!(sse_text.contains("event: member"), "expected a member boundary event before each reply");

    let messages_response = app.oneshot(
        Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie).body(Body::empty()).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut msgs: Vec<_> = tree["messages"].as_object().unwrap().values().collect();
    msgs.sort_by_key(|m| m["created_at"].as_i64().unwrap_or(0));

    // user turn + one reply per enabled member, list order = position order
    assert_eq!(msgs.len(), 3, "user message plus one reply from each of the two enabled members");
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["character_id"], aria_id, "List strategy replies in position order, Aria first");
    assert_eq!(msgs[2]["role"], "assistant");
    assert_eq!(msgs[2]["character_id"], beck_id);
    // each reply is a child of the previous one, not all siblings of the
    // user message ("sequentially, each seeing the previous ones' replies")
    assert_eq!(msgs[2]["parent_id"], msgs[1]["id"]);
    assert_eq!(msgs[1]["parent_id"], msgs[0]["id"]);

    // Beck's prompt must show the user's trigger message before Aria's
    // reply, not after it. build_messages always appends a live
    // `new_user_message` as the LAST turn, so if it were still passed
    // through unchanged on every loop iteration, Beck's prompt would read
    // as Aria replying before the user ever spoke.
    let beck_raw_prompt = msgs[2]["raw_prompt"].as_str().expect("beck's reply should have a stored raw_prompt");
    let prompt_messages: Vec<serde_json::Value> = serde_json::from_str(beck_raw_prompt).unwrap();
    let user_turn_idx = prompt_messages
        .iter()
        .position(|m| m["content"].as_str().unwrap_or("").contains("hi both of you"))
        .expect("beck's prompt should include the user's trigger message");
    let aria_reply_idx = prompt_messages
        .iter()
        .position(|m| m["content"].as_str().unwrap_or("").contains("Aria: reply-0"))
        .expect("beck's prompt should include aria's earlier reply, name-prefixed");
    assert!(
        user_turn_idx < aria_reply_idx,
        "the user's trigger message must appear before Aria's reply in Beck's prompt: {prompt_messages:?}"
    );
}

#[tokio::test]
async fn regenerating_a_groups_reply_only_rerolls_the_named_member() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app().await;

    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    }).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/settings")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(settings_body)).unwrap(),
    ).await.unwrap();

    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;

    let group_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let group_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/groups")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(group_body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(group_response.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let members_body = serde_json::json!({ "members": [
        { "character_id": aria_id, "disabled": false },
        { "character_id": beck_id, "disabled": false },
    ]}).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/groups/{group_id}/members"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(members_body)).unwrap(),
    ).await.unwrap();

    let chat_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/groups/{group_id}/chats"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"title": "Chat"}).to_string())).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let generate_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"content": "hi"}).to_string())).unwrap(),
    ).await.unwrap();
    drain(generate_response).await;

    let messages_response = app.clone().oneshot(
        Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie.clone()).body(Body::empty()).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut msgs: Vec<_> = tree["messages"].as_object().unwrap().values().collect();
    msgs.sort_by_key(|m| m["created_at"].as_i64().unwrap_or(0));
    let user_msg_id = msgs[0]["id"].as_str().unwrap().to_string();
    let aria_reply_id = msgs[1]["id"].as_str().unwrap().to_string();

    // reroll Aria's reply specifically, branching from the user message.
    let regen_response = app.clone().oneshot(
        Request::builder().method("POST")
            .uri(format!("/api/chats/{chat_id}/regenerate?parent_id={user_msg_id}&character_id={aria_id}"))
            .header(header::COOKIE, cookie.clone()).body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(regen_response.status(), StatusCode::OK);
    drain(regen_response).await;

    let messages_response = app.oneshot(
        Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie).body(Body::empty()).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let all_msgs = tree["messages"].as_object().unwrap();

    // user message + old Aria reply + Beck's reply (untouched) + new Aria
    // sibling reply. regenerate must not delete anything or re-run the
    // whole activation strategy
    assert_eq!(all_msgs.len(), 4, "Beck's reply must survive a reroll of Aria, only a new Aria reply gets added");
    let beck_msg = all_msgs.values().find(|m| m["character_id"] == beck_id).expect("Beck's original reply should still exist");
    assert_eq!(beck_msg["parent_id"].as_str(), Some(aria_reply_id.as_str()), "Beck's reply should still be chained under the OLD Aria reply, untouched");
    let aria_msgs: Vec<_> = all_msgs.values().filter(|m| m["character_id"] == aria_id).collect();
    assert_eq!(aria_msgs.len(), 2, "old Aria reply plus one new one");
    let new_reply = aria_msgs.iter().find(|m| m["id"].as_str().unwrap() != aria_reply_id).expect("the regenerated reply should exist");
    assert_eq!(new_reply["parent_id"].as_str(), Some(user_msg_id.as_str()));
}

#[tokio::test]
async fn regenerating_a_non_first_members_reply_names_the_prior_reply_instead_of_treating_it_as_the_user() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app().await;

    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    }).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/settings")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(settings_body)).unwrap(),
    ).await.unwrap();

    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;

    let group_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let group_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/groups")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(group_body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(group_response.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let members_body = serde_json::json!({ "members": [
        { "character_id": aria_id, "disabled": false },
        { "character_id": beck_id, "disabled": false },
    ]}).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri(format!("/api/groups/{group_id}/members"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(members_body)).unwrap(),
    ).await.unwrap();

    let chat_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/groups/{group_id}/chats"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"title": "Chat"}).to_string())).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let generate_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"content": "hi"}).to_string())).unwrap(),
    ).await.unwrap();
    drain(generate_response).await;

    let messages_response = app.clone().oneshot(
        Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie.clone()).body(Body::empty()).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut msgs: Vec<_> = tree["messages"].as_object().unwrap().values().collect();
    msgs.sort_by_key(|m| m["created_at"].as_i64().unwrap_or(0));
    let aria_reply_id = msgs[1]["id"].as_str().unwrap().to_string();
    let beck_reply_id = msgs[2]["id"].as_str().unwrap().to_string();

    // reroll Beck's reply specifically, branching from Aria's reply.
    let regen_response = app.clone().oneshot(
        Request::builder().method("POST")
            .uri(format!("/api/chats/{chat_id}/regenerate?parent_id={aria_reply_id}&character_id={beck_id}"))
            .header(header::COOKIE, cookie.clone()).body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(regen_response.status(), StatusCode::OK);
    drain(regen_response).await;

    let messages_response = app.oneshot(
        Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie).body(Body::empty()).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let all_msgs = tree["messages"].as_object().unwrap();
    let new_beck_reply = all_msgs.values()
        .find(|m| m["character_id"] == beck_id && m["id"].as_str().unwrap() != beck_reply_id)
        .expect("the regenerated Beck reply should exist");
    let raw_prompt = new_beck_reply["raw_prompt"].as_str().unwrap();
    assert!(raw_prompt.contains("Aria: "), "Aria's prior reply must appear as name-prefixed history, not an unlabeled user turn - raw_prompt was: {raw_prompt}");
}

#[tokio::test]
async fn regenerating_a_groups_reply_without_a_character_id_is_rejected() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app().await;
    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    }).to_string();
    app.clone().oneshot(
        Request::builder().method("PUT").uri("/api/settings")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(settings_body)).unwrap(),
    ).await.unwrap();

    let group_body = serde_json::json!({ "name": "Solo Group", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let group_response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/groups")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(group_body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(group_response.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let chat_response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/groups/{group_id}/chats"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.clone())
            .body(Body::from(serde_json::json!({"title": "Chat"}).to_string())).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let regen_response = app.oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/regenerate"))
            .header(header::COOKIE, cookie).body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(regen_response.status(), StatusCode::BAD_REQUEST);
}
