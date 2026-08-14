mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn empty_persona_list_for_new_user() {
    let (app, cookie) = common::authed_app().await;

    let list = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/personas")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(list.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_update_delete_persona_and_activate() {
    let (app, cookie) = common::authed_app().await;

    let create_body = serde_json::json!({"name": "Detective OC", "description": "A 190cm cyborg detective."}).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/personas")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created_bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let created_persona: serde_json::Value = serde_json::from_slice(&created_bytes).unwrap();
    let id = created_persona["id"].as_str().unwrap().to_string();

    let activate_body = serde_json::json!({"persona_id": id}).to_string();
    let activated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/personas/active")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(activate_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(activated.status(), StatusCode::OK);

    let me = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let me_bytes = axum::body::to_bytes(me.into_body(), usize::MAX).await.unwrap();
    let me_json: serde_json::Value = serde_json::from_slice(&me_bytes).unwrap();
    assert_eq!(me_json["active_persona_id"], id);

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/personas/{id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);

    let me_after = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let me_after_bytes = axum::body::to_bytes(me_after.into_body(), usize::MAX).await.unwrap();
    let me_after_json: serde_json::Value = serde_json::from_slice(&me_after_bytes).unwrap();
    assert!(me_after_json["active_persona_id"].is_null());
}

#[tokio::test]
async fn cannot_edit_or_delete_another_users_persona() {
    let (app, cookie_a, cookie_b) = common::authed_app_with_second_user().await;

    let create_body = serde_json::json!({"name": "User A's persona", "description": ""}).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/personas")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie_a.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let created_bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let created_persona: serde_json::Value = serde_json::from_slice(&created_bytes).unwrap();
    let id = created_persona["id"].as_str().unwrap().to_string();

    let delete_as_b = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/personas/{id}"))
                .header(header::COOKIE, cookie_b.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_as_b.status(), StatusCode::NOT_FOUND);
    // this used to be a bodiless 404 - make sure it's a real JSON error now,
    // since the frontend reads resp.text() on failure and an empty body
    // renders as no error at all
    let delete_as_b_bytes = axum::body::to_bytes(delete_as_b.into_body(), usize::MAX).await.unwrap();
    let delete_as_b_json: serde_json::Value = serde_json::from_slice(&delete_as_b_bytes).unwrap();
    assert!(!delete_as_b_json["message"].as_str().unwrap_or_default().is_empty());
}
