mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

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

async fn create_character_with_greeting(app: &axum::Router, cookie: &str, name: &str, greeting: &str) -> String {
    let body = serde_json::json!({ "name": name, "first_message": greeting }).to_string();
    let response = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string()
}

async fn create_chat(app: &axum::Router, cookie: &str, character_id: &str) -> serde_json::Value {
    let body = serde_json::json!({ "title": "Test" }).to_string();
    let response = app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/characters/{character_id}/chats"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn add_member(app: &axum::Router, cookie: &str, chat_id: &str, character_id: &str) -> axum::http::Response<Body> {
    let body = serde_json::json!({ "character_id": character_id }).to_string();
    app.clone().oneshot(
        Request::builder().method("POST").uri(format!("/api/chats/{chat_id}/members"))
            .header(header::CONTENT_TYPE, "application/json").header(header::COOKIE, cookie)
            .body(Body::from(body)).unwrap(),
    ).await.unwrap()
}

async fn remove_member(app: &axum::Router, cookie: &str, chat_id: &str, character_id: &str) -> axum::http::Response<Body> {
    app.clone().oneshot(
        Request::builder().method("DELETE").uri(format!("/api/chats/{chat_id}/members/{character_id}"))
            .header(header::COOKIE, cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap()
}

async fn json_body(response: axum::http::Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn list_chats(app: &axum::Router, cookie: &str, character_id: &str) -> Vec<serde_json::Value> {
    let response = app.clone().oneshot(
        Request::builder().method("GET").uri(format!("/api/characters/{character_id}/chats"))
            .header(header::COOKIE, cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    json_body(response).await.as_array().cloned().unwrap()
}

async fn list_messages(app: &axum::Router, cookie: &str, chat_id: &str) -> serde_json::Value {
    let response = app.clone().oneshot(
        Request::builder().method("GET").uri(format!("/api/chats/{chat_id}/messages"))
            .header(header::COOKIE, cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    json_body(response).await
}

#[tokio::test]
async fn adding_a_second_member_converts_a_direct_chat_into_a_group() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();

    let response = add_member(&app, &cookie, chat_id, &beck_id).await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated = json_body(response).await;
    assert!(updated["character_id"].is_null());
    assert!(updated["group_id"].is_string());
}

#[tokio::test]
async fn adding_a_third_member_appends_without_disturbing_existing_ones() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;
    let cass_id = create_character(&app, &cookie, "Cass").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();

    let after_second = json_body(add_member(&app, &cookie, chat_id, &beck_id).await).await;
    let group_id = after_second["group_id"].as_str().unwrap().to_string();

    let response = add_member(&app, &cookie, chat_id, &cass_id).await;
    assert_eq!(response.status(), StatusCode::OK);
    let after_third = json_body(response).await;
    assert_eq!(
        after_third["group_id"].as_str(),
        Some(group_id.as_str()),
        "adding a third member must reuse the same group, not create a new one"
    );

    let repeat = add_member(&app, &cookie, chat_id, &cass_id).await;
    assert_eq!(repeat.status(), StatusCode::CONFLICT, "adding an already-present member should be rejected, not silently accepted");
}

#[tokio::test]
async fn removing_down_to_one_member_converts_back_to_a_direct_chat() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();
    add_member(&app, &cookie, chat_id, &beck_id).await;

    let response = remove_member(&app, &cookie, chat_id, &beck_id).await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated = json_body(response).await;
    assert_eq!(updated["character_id"].as_str(), Some(aria_id.as_str()));
    assert!(updated["group_id"].is_null());
}

#[tokio::test]
async fn removing_the_last_member_is_rejected() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();

    let response = remove_member(&app, &cookie, chat_id, &aria_id).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn adding_someone_elses_character_is_rejected() {
    // two authed_app() calls would spin up two separate in-memory DBs, and a
    // session cookie from one doesn't authenticate against the other's
    // sessions table, so authed_app_with_second_user() puts both users in the
    // same DB behind the same app, which is what a real cross-tenant check
    // needs.
    let (app, cookie, other_cookie) = common::authed_app_with_second_user().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();
    let other_users_character = create_character(&app, &other_cookie, "Not Yours").await;

    let response = add_member(&app, &cookie, chat_id, &other_users_character).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn adding_the_chats_own_character_as_a_member_is_rejected() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();

    let response = add_member(&app, &cookie, chat_id, &aria_id).await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "adding the chat's own character back as a member should be rejected, not 500 after leaking a group row"
    );
}

#[tokio::test]
async fn a_chat_that_becomes_a_group_still_shows_up_in_both_members_chat_lists() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap().to_string();

    add_member(&app, &cookie, &chat_id, &beck_id).await;

    let aria_chats = list_chats(&app, &cookie, &aria_id).await;
    assert!(
        aria_chats.iter().any(|c| c["id"].as_str() == Some(chat_id.as_str())),
        "the chat should still show up for the character who started it, not vanish once it becomes a group"
    );

    let beck_chats = list_chats(&app, &cookie, &beck_id).await;
    assert!(
        beck_chats.iter().any(|c| c["id"].as_str() == Some(chat_id.as_str())),
        "the chat should also show up for the character who was added to the group"
    );
}

#[tokio::test]
async fn adding_a_second_member_backfills_existing_messages_to_the_original_character() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character_with_greeting(&app, &cookie, "Aria", "Hey there!").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;
    let chat = create_chat(&app, &cookie, &aria_id).await;
    let chat_id = chat["id"].as_str().unwrap();

    add_member(&app, &cookie, chat_id, &beck_id).await;

    let tree = list_messages(&app, &cookie, chat_id).await;
    let messages = tree["messages"].as_object().unwrap();
    let greeting = messages
        .values()
        .find(|m| m["role"] == "assistant")
        .expect("the greeting message should still be in the tree after conversion");
    assert_eq!(
        greeting["character_id"].as_str(),
        Some(aria_id.as_str()),
        "a pre-conversion message should be backfilled to the character who actually said it, not left unattributed"
    );
}
