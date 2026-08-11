mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

async fn create_test_character(app: &axum::Router, cookie: &str) -> String {
    let body = serde_json::json!({
        "name": "Seraphina", "description": "", "personality": "", "scenario": "", "first_message": "Hi."
    }).to_string();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_chat_add_and_delete_messages() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "First chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_chat.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    let messages_resp = app
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
    assert_eq!(messages_resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(messages_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // response is now a tree: { "root_id": "...", "messages": { ... } }
    let messages_map = tree["messages"].as_object().unwrap();
    assert_eq!(messages_map.len(), 1, "should have one message (the greeting)");
    let first_msg = messages_map.values().next().unwrap();
    assert_eq!(first_msg["role"], "assistant");
    assert_eq!(first_msg["content"], "Hi.");
}

#[tokio::test]
async fn creating_a_chat_persists_the_characters_first_message_as_the_opening_reply() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Greeting check"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_chat.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    let messages_resp = app
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
    assert_eq!(messages_resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(messages_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let messages_map = tree["messages"].as_object().unwrap();

    assert_eq!(messages_map.len(), 1, "the greeting should be the chat's only message right after creation");
    let first_msg = messages_map.values().next().unwrap();
    assert_eq!(first_msg["role"], "assistant");
    assert_eq!(first_msg["content"], "Hi.");
}

#[tokio::test]
async fn creating_a_chat_for_a_character_with_no_greeting_starts_with_an_empty_message_list() {
    let (app, cookie) = common::authed_app().await;
    let body = serde_json::json!({
        "name": "No Greeting", "description": "", "personality": "", "scenario": "", "first_message": ""
    })
    .to_string();
    let create_character = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_character.into_body(), usize::MAX).await.unwrap();
    let character_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let character_id = character_json["id"].as_str().unwrap().to_string();

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "No greeting chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    let messages_resp = app
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
    let bytes = axum::body::to_bytes(messages_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let messages_map = tree["messages"].as_object().unwrap();
    assert_eq!(
        messages_map.len(),
        0,
        "a character with no first_message should not get a phantom empty greeting message"
    );
}

#[tokio::test]
async fn delete_message_missing_id_returns_404() {
    let (app, cookie) = common::authed_app().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/messages/does-not-exist")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tree_lists_messages_with_proper_structure() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Tree chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    let tree_resp = app
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
    assert_eq!(tree_resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert!(tree["root_id"].is_string(), "tree should have a root_id");
    assert!(tree["messages"].is_object(), "tree should have a messages map");
    let messages = tree["messages"].as_object().unwrap();
    let root = messages.values().next().unwrap();
    assert_eq!(root["parent_id"], serde_json::Value::Null);
    assert!(root["children"].is_array(), "node should have a children list");
    assert_eq!(root["role"], "assistant");
}

#[tokio::test]
async fn edit_message_updates_user_content() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Edit chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    // get the greeting message id.
    let tree_resp = app
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
    let bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let greeting_id = tree["messages"].as_object().unwrap().values().next().unwrap()["id"]
        .as_str().unwrap().to_string();

    // editing an assistant message's wording (without a full regenerate) is
    // a normal roleplay-editor feature, same as editing your own message.
    let edit_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/messages/{greeting_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"content": "edited greeting"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(edit_resp.status(), StatusCode::OK, "editing an assistant message should be allowed");

    let tree_resp = app
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
    let bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(tree["messages"][&greeting_id]["content"], "edited greeting");
}

#[tokio::test]
async fn soft_delete_message_reparents_children_and_removes_from_tree() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Delete chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    // get the greeting id.
    let tree_resp = app
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
    let bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let greeting_id = tree["messages"].as_object().unwrap().values().next().unwrap()["id"]
        .as_str().unwrap().to_string();

    // soft-delete the greeting. since it's the root with no children, this
    // should just mark it deleted = 1 and it disappears from the tree.
    let delete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/messages/{greeting_id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);

    // tree should now be empty.
    let tree_resp = app
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
    let bytes = axum::body::to_bytes(tree_resp.into_body(), usize::MAX).await.unwrap();
    let tree: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(tree["root_id"].is_null(), "tree should have no root after deleting the only message");
}

#[tokio::test]
async fn sending_empty_message_returns_400() {
    let (app, cookie) = common::authed_app().await;
    let character_id = create_test_character(&app, &cookie).await;

    let create_chat = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/characters/{character_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Empty msg chat"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(create_chat.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let chat_id = chat_json["id"].as_str().unwrap().to_string();

    // sending an empty/whitespace-only message should be rejected with 400.
    let empty_body = serde_json::json!({"content": "   ", "parent_id": null}).to_string();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/chats/{chat_id}/generate"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(empty_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
