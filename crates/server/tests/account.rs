mod common;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
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
