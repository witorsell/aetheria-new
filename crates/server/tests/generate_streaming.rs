mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::routing::post;
use futures_util::{stream, StreamExt};
use http_body_util::BodyExt;
use std::future::IntoFuture;
use std::time::{Duration, Instant};
use tower::ServiceExt;

/// a minimal mock OpenAI-compatible server that streams three separate
/// chunks with real delays between them, so the test can prove the client
/// receives them incrementally rather than as one buffered response.
async fn spawn_mock_provider() -> String {
    async fn mock_completions() -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
        // `Event::data()` already writes the `data: ` prefix (and the blank
        // line separator) itself, so the chunks here are the raw payloads,
        // not pre-formatted SSE lines, otherwise it double-wraps them.
        let chunks = vec![
            "{\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}".to_string(),
            "{\"choices\":[{\"delta\":{\"content\":\", world\"}}]}".to_string(),
            "{\"choices\":[{\"delta\":{\"content\":\"!\"}}]}".to_string(),
            "[DONE]".to_string(),
        ];
        let stream = stream::iter(chunks).then(|chunk| async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            Ok(Event::default().data(chunk))
        });
        Sse::new(stream)
    }

    let app = axum::Router::new().route("/chat/completions", post(mock_completions));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    format!("http://{addr}")
}

#[tokio::test]
async fn generate_streams_the_reply_incrementally_and_persists_it() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;

    // point settings at the mock provider.
    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url,
        "api_key": "test-key",
        "model_name": "test-model",
        "system_prompt": "",
        "context_limit": 8192,
        "post_history_instructions": "",
        "forbid_external_media": false,
        "provider_type": "openai"
    })
    .to_string();
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(settings_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let character_body = serde_json::json!({
        "name": "Seraphina", "description": "", "personality": "", "scenario": "", "first_message": ""
    }).to_string();
    let character_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(character_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(character_response.into_body(), usize::MAX).await.unwrap();
    let character_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let chat_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let generate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/chats/{chat_id}/generate"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generate_response.status(), StatusCode::OK);

    // read the SSE body frame by frame instead of `to_bytes`, which would
    // drain the whole response before any assertion runs. draining first
    // can only prove the final buffered text contains three `data:` lines,
    // it can't tell "delivered to the client incrementally, in real time"
    // apart from "collected internally by the handler and flushed all at
    // once at the end", both produce an identical final body. timestamping
    // each frame as it arrives lets the test check the actual wall-clock
    // spacing between them.
    let mut body = generate_response.into_body();
    let mut collected = Vec::new();
    let mut frame_arrival_times = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.expect("reading an SSE frame should not fail");
        if let Some(data) = frame.data_ref() {
            if !data.is_empty() {
                frame_arrival_times.push(Instant::now());
                collected.extend_from_slice(data);
            }
        }
    }
    let body_text = String::from_utf8(collected).unwrap();

    // all three chunks arrived, in order, as separate SSE events.
    assert!(body_text.contains("Hello"));
    assert!(body_text.contains(", world"));
    assert!(body_text.contains("!"));

    // prove genuine incremental delivery. the mock upstream sleeps 20ms
    // before each of its 4 chunks, so a handler that truly streams each
    // delta through to the client as it arrives sees real wall-clock
    // spacing between the first and last content-bearing frame: about
    // 40ms between the first chunk ("hello", sent at ~20ms) and the third
    // ("!", sent at ~60ms). a handler that buffers the whole provider
    // stream internally before writing anything to the response, the
    // nginx/server-buffering bug class this project's rewrite exists to
    // avoid, would instead deliver every frame back-to-back once the full
    // reply is ready, with a near-zero gap between them. 30ms is comfortably
    // below the ~40ms expected spread but far above what a buffered
    // implementation could produce.
    assert!(
        frame_arrival_times.len() >= 3,
        "expected at least 3 separate content frames, got {}",
        frame_arrival_times.len()
    );
    let first_frame = *frame_arrival_times.first().unwrap();
    let last_frame = *frame_arrival_times.last().unwrap();
    let spread = last_frame.duration_since(first_frame);
    assert!(
        spread >= Duration::from_millis(30),
        "frames arrived only {spread:?} apart; expected close to the mock's \
         ~40ms spacing between its first and third chunk, a near-zero gap \
         would mean the response was buffered and flushed all at once \
         instead of streamed incrementally"
    );

    // the completed assistant message was persisted.
    let messages_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/chats/{chat_id}/messages"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let messages_map = tree["messages"].as_object().unwrap();
    let messages: Vec<_> = messages_map.values().collect();
    assert_eq!(messages.len(), 2, "the user message and the completed assistant reply are both persisted");
    // sort by created_at to get ordering.
    let mut msgs = messages.clone();
    msgs.sort_by_key(|m| m["created_at"].as_i64().unwrap_or(0));
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"], "Hi!");
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["content"], "Hello, world!");
}

/// drains an SSE response body to completion (needed so the handler's
/// persistence step, which runs after the stream ends, has actually happened
/// by the time the test inspects the database).
async fn drain(response: axum::http::Response<Body>) {
    let mut body = response.into_body();
    while body.frame().await.is_some() {}
}

#[tokio::test]
async fn regenerate_branches_from_the_given_parent_instead_of_the_last_reply() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;

    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url,
        "api_key": "test-key",
        "model_name": "test-model",
        "system_prompt": "",
        "context_limit": 8192,
        "post_history_instructions": "",
        "forbid_external_media": false,
        "provider_type": "openai"
    })
    .to_string();
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(settings_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let character_body = serde_json::json!({
        "name": "Seraphina", "description": "", "personality": "", "scenario": "", "first_message": ""
    }).to_string();
    let character_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(character_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(character_response.into_body(), usize::MAX).await.unwrap();
    let character_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let chat_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let generate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/chats/{chat_id}/generate"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generate_response.status(), StatusCode::OK);
    drain(generate_response).await;

    let messages_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/chats/{chat_id}/messages"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let messages_map = tree["messages"].as_object().unwrap();
    let mut msgs: Vec<_> = messages_map.values().collect();
    msgs.sort_by_key(|m| m["created_at"].as_i64().unwrap_or(0));
    let user_msg_id = msgs[0]["id"].as_str().unwrap().to_string();
    let first_reply_id = msgs[1]["id"].as_str().unwrap().to_string();

    // regenerate, explicitly branching from the user message rather than
    // whatever the active branch happens to end on.
    let regenerate_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/chats/{chat_id}/regenerate?parent_id={user_msg_id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(regenerate_response.status(), StatusCode::OK);
    drain(regenerate_response).await;

    let messages_response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/chats/{chat_id}/messages"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(messages_response.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let messages_map = tree["messages"].as_object().unwrap();

    assert_eq!(messages_map.len(), 3, "user message plus two sibling replies");
    let second_reply = messages_map
        .values()
        .find(|m| m["id"].as_str().unwrap() != user_msg_id && m["id"].as_str().unwrap() != first_reply_id)
        .expect("the regenerated reply should exist");

    assert_eq!(
        second_reply["parent_id"].as_str(),
        Some(user_msg_id.as_str()),
        "the new reply should branch from the user message, not nest under the old reply"
    );
    assert_eq!(
        second_reply["content"], "Hello, world!",
        "the mock always replies the same way; a bug that sends the old \
         reply's own text back as the \"new user message\" would still \
         produce this same content, but the parent_id assertion above is \
         what actually catches that"
    );
}


/// settings pointed at the mock provider, one character and one chat. returns the chat id.
async fn setup_chat(app: &axum::Router, cookie: &str, mock_base_url: &str) -> String {
    let settings_body = serde_json::json!({
        "api_base_url": mock_base_url, "api_key": "test-key", "model_name": "test-model",
        "system_prompt": "", "context_limit": 8192, "post_history_instructions": "",
        "forbid_external_media": false, "provider_type": "openai"
    })
    .to_string();
    app.clone().oneshot(Request::builder().method("PUT").uri("/api/settings")
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
        .body(Body::from(settings_body)).unwrap()).await.unwrap();
    let character = app.clone().oneshot(Request::builder().method("POST").uri("/api/characters")
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
        .body(Body::from(serde_json::json!({"name": "Seraphina", "description": "", "personality": "", "scenario": "", "first_message": ""}).to_string()))
        .unwrap()).await.unwrap();
    let bytes = axum::body::to_bytes(character.into_body(), usize::MAX).await.unwrap();
    let character_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();
    let chat = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/characters/{character_id}/chats"))
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
        .body(Body::from(serde_json::json!({"title": "Chat"}).to_string())).unwrap()).await.unwrap();
    let bytes = axum::body::to_bytes(chat.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string()
}

async fn poll_events(app: &axum::Router, cookie: &str, chat_id: &str, generation_id: &str, from: usize) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(Request::builder()
        .uri(format!("/api/chats/{chat_id}/generation/{generation_id}/events?from={from}&wait=3000"))
        .header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn a_client_that_never_reads_the_stream_can_replay_the_reply_from_the_event_log() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;
    let chat_id = setup_chat(&app, &cookie, &mock_base_url).await;

    let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.as_str())
        .header("x-generation-id", "replay-test-0001")
        .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // the live connection froze for this client: it never reads a byte of it
    drop(response);

    let mut text = String::new();
    let mut from = 0;
    let mut saw_done = false;
    for _ in 0..20 {
        let (status, body) = poll_events(&app, &cookie, &chat_id, "replay-test-0001", from).await;
        assert_eq!(status, StatusCode::OK);
        for event in body["events"].as_array().unwrap() {
            match event["k"].as_str().unwrap() {
                "data" => text.push_str(event["d"].as_str().unwrap()),
                "done" => saw_done = true,
                other => panic!("unexpected event kind {other}"),
            }
        }
        from = body["next"].as_u64().unwrap() as usize;
        if body["done"].as_bool().unwrap() {
            break;
        }
    }
    assert_eq!(text, "Hello, world!");
    assert!(saw_done, "the log should end with the done event");

    // the reply was saved by the generation task even though nobody read the stream
    let messages = app.clone().oneshot(Request::builder().uri(format!("/api/chats/{chat_id}/messages"))
        .header(header::COOKIE, cookie.as_str()).body(Body::empty()).unwrap()).await.unwrap();
    let bytes = axum::body::to_bytes(messages.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("Hello, world!"));
}

#[tokio::test]
async fn the_event_log_can_be_read_again_from_any_index() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;
    let chat_id = setup_chat(&app, &cookie, &mock_base_url).await;
    let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.as_str())
        .header("x-generation-id", "index-test-0001")
        .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string())).unwrap()).await.unwrap();
    drop(response);

    let (_, all) = poll_events(&app, &cookie, &chat_id, "index-test-0001", 0).await;
    let mut total = all["events"].as_array().unwrap().len();
    let mut next = all["next"].as_u64().unwrap() as usize;
    while !all["done"].as_bool().unwrap() && next < 5 {
        let (_, more) = poll_events(&app, &cookie, &chat_id, "index-test-0001", next).await;
        total += more["events"].as_array().unwrap().len();
        next = more["next"].as_u64().unwrap() as usize;
        if more["done"].as_bool().unwrap() { break; }
    }
    // three text events plus done
    assert_eq!(total, 4);
    let (_, tail) = poll_events(&app, &cookie, &chat_id, "index-test-0001", 2).await;
    let kinds: Vec<_> = tail["events"].as_array().unwrap().iter().map(|e| e["k"].as_str().unwrap().to_string()).collect();
    assert_eq!(kinds, ["data", "done"]);
    assert_eq!(tail["done"], true);
}

#[tokio::test]
async fn the_event_log_belongs_to_the_user_and_chat_that_started_it() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, owner, other) = common::authed_app_with_second_user_and_providers(&[mock_base_url.as_str()]).await;
    let chat_id = setup_chat(&app, &owner, &mock_base_url).await;
    let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, owner.as_str())
        .header("x-generation-id", "owner-test-0001")
        .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string())).unwrap()).await.unwrap();
    drop(response);

    assert_eq!(poll_events(&app, &owner, &chat_id, "owner-test-0001", 0).await.0, StatusCode::OK);
    assert_eq!(poll_events(&app, &other, &chat_id, "owner-test-0001", 0).await.0, StatusCode::NOT_FOUND);
    assert_eq!(poll_events(&app, &owner, "some-other-chat", "owner-test-0001", 0).await.0, StatusCode::NOT_FOUND);
    assert_eq!(poll_events(&app, &owner, &chat_id, "never-registered-1", 0).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_request_without_a_usable_generation_id_still_streams_as_before() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;
    let chat_id = setup_chat(&app, &cookie, &mock_base_url).await;
    for id in [None, Some("x"), Some("has spaces in it")] {
        let mut request = Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.as_str());
        if let Some(id) = id { request = request.header("x-generation-id", id); }
        let response = app.clone().oneshot(request.body(Body::from(serde_json::json!({"content": "Hi!"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("event: done"));
    }
}

#[tokio::test]
async fn live_events_carry_their_log_position_as_the_sse_id() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;
    let chat_id = setup_chat(&app, &cookie, &mock_base_url).await;
    let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
        .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.as_str())
        .header("x-generation-id", "sse-id-test-0001")
        .body(Body::from(serde_json::json!({"content": "Hi!"}).to_string())).unwrap()).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for position in 0..4 {
        assert!(text.contains(&format!("id: {position}")), "missing id {position} in {text:?}");
    }
    assert!(!text.contains("id: 4"));
}

async fn body_of(app: &axum::Router, cookie: &str, uri: &str) -> Vec<u8> {
    let response = app.clone().oneshot(Request::builder().uri(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()
}

/// the Cloudflare worker in front of the site fetches a long response as separate
/// byte-range requests and glues the pieces together, so every request for the same
/// data has to return the same bytes. a chat used to come out in a different message
/// order each time, and the glued result lost messages or failed to parse
#[tokio::test]
async fn a_long_chat_comes_back_as_identical_bytes_every_time() {
    let mock_base_url = spawn_mock_provider().await;
    let (app, cookie) = common::authed_app_for_provider(&mock_base_url).await;
    let chat_id = setup_chat(&app, &cookie, &mock_base_url).await;

    let mut parent: Option<String> = None;
    for i in 0..4 {
        let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/generate"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie.as_str())
            .body(Body::from(serde_json::json!({"content": format!("message {i}"), "parent_id": parent}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let tree: serde_json::Value = serde_json::from_slice(&body_of(&app, &cookie, &format!("/api/chats/{chat_id}/messages?brief=true")).await).unwrap();
        parent = tree["messages"].as_object().unwrap().values()
            .max_by_key(|m| m["created_at"].as_i64().unwrap()).map(|m| m["id"].as_str().unwrap().to_string());
    }

    let uri = format!("/api/chats/{chat_id}/messages?brief=true");
    let first = body_of(&app, &cookie, &uri).await;
    let tree: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(tree["messages"].as_object().unwrap().len(), 8);
    for _ in 0..15 {
        assert_eq!(body_of(&app, &cookie, &uri).await, first, "history bytes changed between requests");
    }

    let root = tree["root_id"].as_str().unwrap();
    let subtree_uri = format!("/api/chats/{chat_id}/messages/tree?from={root}&depth=20");
    let first = body_of(&app, &cookie, &subtree_uri).await;
    for _ in 0..15 {
        assert_eq!(body_of(&app, &cookie, &subtree_uri).await, first, "subtree bytes changed between requests");
    }
}
