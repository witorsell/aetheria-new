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
    let (app, cookie) = common::authed_app().await;

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
    let (app, cookie) = common::authed_app().await;

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
