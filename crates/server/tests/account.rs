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

#[tokio::test]
async fn nuclear_delete_wipes_content_for_that_user_only() {
    let (app, cookie_a, cookie_b) = common::authed_app_with_second_user().await;

    let char_a = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_a.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"A's Character"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(char_a.status(), StatusCode::OK);

    let char_b = app.clone().oneshot(
        Request::builder().method("POST").uri("/api/characters")
            .header(header::COOKIE, cookie_b.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"name":"B's Character"}"#)).unwrap(),
    ).await.unwrap();
    assert_eq!(char_b.status(), StatusCode::OK);

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

    let b_export = app.clone().oneshot(
        Request::builder().method("GET").uri("/api/account/export-all")
            .header(header::COOKIE, cookie_b.clone())
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    let b_bytes = axum::body::to_bytes(b_export.into_body(), usize::MAX).await.unwrap();
    let b: server::models::account::AccountExport = serde_json::from_slice(&b_bytes).unwrap();
    assert_eq!(b.characters.len(), 1);
    assert_eq!(b.characters[0].name, "B's Character");
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
