mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn update_and_fetch_settings_never_echoes_the_raw_key() {
    let (app, cookie) = common::authed_app().await;

    let update_body = serde_json::json!({
        "api_base_url": "https://nano-gpt.com/api/v1",
        "api_key": "sk-secret-value",
        "model_name": "gpt-4o-mini",
        "system_prompt": "Be concise.",
        "context_limit": 8192,
        "post_history_instructions": "",
        "forbid_external_media": false,
        "provider_type": "openai"
    })
    .to_string();

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);

    let fetch = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(fetch.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["api_base_url"], "https://nano-gpt.com/api/v1");
    assert_eq!(json["has_api_key"], true);
    assert!(json.get("api_key").is_none(), "the raw key must never be echoed back");
}

#[tokio::test]
async fn resaving_settings_without_a_new_key_does_not_wipe_the_stored_key() {
    let (app, cookie) = common::authed_app().await;

    let first_update = serde_json::json!({
        "api_base_url": "https://nano-gpt.com/api/v1",
        "api_key": "sk-secret-value",
        "model_name": "gpt-4o-mini",
        "system_prompt": "Be concise.",
        "context_limit": 8192,
        "post_history_instructions": "",
        "forbid_external_media": false,
        "provider_type": "openai"
    })
    .to_string();

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(first_update))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second_update = serde_json::json!({
        "api_base_url": "https://nano-gpt.com/api/v1",
        "api_key": null,
        "model_name": "gpt-4o",
        "system_prompt": "Be concise.",
        "context_limit": 8192,
        "post_history_instructions": "",
        "forbid_external_media": false,
        "provider_type": "openai"
    })
    .to_string();

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(second_update))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);

    let fetch = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(fetch.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["model_name"], "gpt-4o", "the second update's other fields should apply");
    assert_eq!(
        json["has_api_key"], true,
        "omitting api_key on a later update must not wipe the previously stored key"
    );
}
