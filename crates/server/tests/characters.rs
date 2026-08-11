mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::multipart_body;
use tower::ServiceExt;

#[tokio::test]
async fn create_list_update_delete_character() {
    let (app, cookie) = common::authed_app().await;

    let create_body = serde_json::json!({
        "name": "Seraphina",
        "description": "A forest guardian.",
        "personality": "Warm, curious.",
        "scenario": "A quiet glade.",
        "first_message": "Hello there, traveler."
    })
    .to_string();

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created_bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let created_char: serde_json::Value = serde_json::from_slice(&created_bytes).unwrap();
    let id = created_char["id"].as_str().unwrap().to_string();
    assert_eq!(created_char["name"], "Seraphina");

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/characters")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let list_bytes = axum::body::to_bytes(list.into_body(), usize::MAX).await.unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&list_bytes).unwrap();
    assert_eq!(list_json.as_array().unwrap().len(), 1);

    let update_body = serde_json::json!({
        "name": "Seraphina the Wise",
        "description": "A forest guardian.",
        "personality": "Warm, curious.",
        "scenario": "A quiet glade.",
        "first_message": "Hello there, traveler."
    })
    .to_string();
    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/characters/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let deleted = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/characters/{id}"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
}

#[tokio::test]
async fn update_and_delete_on_missing_id_return_not_found() {
    let (app, cookie) = common::authed_app().await;

    let update_body = serde_json::json!({
        "name": "Ghost",
        "description": "Doesn't exist.",
        "personality": "N/A",
        "scenario": "N/A",
        "first_message": "N/A"
    })
    .to_string();

    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/characters/does-not-exist")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::NOT_FOUND);

    let deleted = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/characters/does-not-exist")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn importing_a_character_file_over_2mb_is_not_rejected_as_no_file_uploaded() {
    // real exported character PNG cards (embedded avatar plus a base64 JSON
    // metadata chunk) routinely land in the 3-4MB range, well past axum's
    // 2MB default body limit. below that limit, multipart parsing errors out
    // mid-stream and the handler's old `.unwrap_or(None)` swallowed the
    // error, surfacing the misleading "No file uploaded" instead of the real
    // cause. this sends a plain (non-PNG) JSON payload padded past 2MB,
    // since the handler falls back to parsing raw JSON when PNG decoding
    // fails, so the size is what's under test, not the PNG parsing path.
    let (app, cookie) = common::authed_app().await;

    let padding = "x".repeat(3 * 1024 * 1024);
    let character_json = serde_json::json!({
        "name": "Big Card",
        "description": padding,
        "spec": "chara_card_v2",
        "spec_version": "2.0"
    })
    .to_string();
    assert!(character_json.len() > 2 * 1024 * 1024, "test payload must actually exceed the old 2MB default");

    let (content_type, body) = multipart_body("file", "character.json", "application/json", character_json.as_bytes());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import/character")
                .header(header::CONTENT_TYPE, content_type)
                .header(header::COOKIE, cookie)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["name"], "Big Card");
}

#[tokio::test]
async fn creating_character_with_empty_name_returns_400() {
    let (app, cookie) = common::authed_app().await;
    let create_body = serde_json::json!({ "name": "   ", "description": "test" }).to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn setting_avatar_url_to_javascript_scheme_returns_400() {
    let (app, cookie) = common::authed_app().await;
    let create_body = serde_json::json!({
        "name": "Hacker",
        "avatar_url": "javascript:alert(1)"
    })
    .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn creating_character_with_oversized_description_returns_400() {
    let (app, cookie) = common::authed_app().await;
    let big_desc = "x".repeat(100_001);
    let create_body = serde_json::json!({ "name": "Blurb", "description": big_desc }).to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn fetching_another_users_character_returns_404() {
    let (app, first_cookie, second_cookie) = common::authed_app_with_second_user().await;

    // user 1 creates a character
    let create_body = serde_json::json!({ "name": "Private" }).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/characters")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_cookie)
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = json["id"].as_str().unwrap().to_string();

    // user 2 tries to fetch it, should get 404
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/api/characters/{id}"))
                .header(header::COOKIE, second_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

