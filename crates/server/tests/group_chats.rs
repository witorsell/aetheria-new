mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

async fn create_character(app: &axum::Router, cookie: &str, name: &str) -> String {
    let body = serde_json::json!({ "name": name }).to_string();
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
    serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn creating_a_chat_in_a_group_sets_group_id_not_character_id() {
    let (app, cookie) = common::authed_app().await;

    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;

    let group_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let group_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(group_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(group_response.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let members_body = serde_json::json!({
        "members": [
            { "character_id": aria_id, "disabled": false },
            { "character_id": beck_id, "disabled": false },
        ]
    }).to_string();
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}/members"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(members_body))
                .unwrap(),
        )
        .await
        .unwrap();

    let chat_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/groups/{group_id}/chats"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(serde_json::json!({"title": "Study session"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(chat_response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(chat_response.into_body(), usize::MAX).await.unwrap();
    let chat_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(chat_json["group_id"], group_id);
    assert!(chat_json["character_id"].is_null());

    // creating a chat under a group that doesn't exist (or belongs to
    // another user) 404s instead of silently inserting an orphaned row.
    let missing_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups/does-not-exist/chats")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(serde_json::json!({"title": "x"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);
}
