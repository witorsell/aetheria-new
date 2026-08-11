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
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn create_list_get_update_delete_a_group() {
    let (app, cookie) = common::authed_app().await;

    let create_body = serde_json::json!({
        "name": "Study Club",
        "avatar_url": null,
        "activation_strategy": "natural"
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let created_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let group_id = created_json["id"].as_str().unwrap().to_string();
    assert_eq!(created_json["name"], "Study Club");
    assert_eq!(created_json["activation_strategy"], "natural");

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/groups")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(list_response.into_body(), usize::MAX).await.unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(list_json.as_array().unwrap().len(), 1);

    let update_body = serde_json::json!({ "name": "Renamed Club", "avatar_url": null, "activation_strategy": "list" }).to_string();
    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let fetched = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetched.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(fetched.into_body(), usize::MAX).await.unwrap();
    let fetched_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fetched_json["name"], "Renamed Club");
    assert_eq!(fetched_json["members"].as_array().unwrap().len(), 0);

    let deleted = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);

    let fetched_after_delete = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(fetched_after_delete.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn set_members_orders_them_and_get_reflects_it() {
    let (app, cookie) = common::authed_app().await;
    let aria_id = create_character(&app, &cookie, "Aria").await;
    let beck_id = create_character(&app, &cookie, "Beck").await;

    let create_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": null }).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    let members_body = serde_json::json!({
        "members": [
            { "character_id": beck_id, "disabled": true },
            { "character_id": aria_id, "disabled": false }
        ]
    })
    .to_string();
    let set_response = app
        .clone()
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
    assert_eq!(set_response.status(), StatusCode::OK);

    let fetched = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(fetched.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let members = json["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["character_id"], beck_id, "Beck was listed first in the request, so should be position 0");
    assert_eq!(members[0]["disabled"], true);
    assert_eq!(members[1]["character_id"], aria_id);
    assert_eq!(members[1]["disabled"], false);
}

#[tokio::test]
async fn operations_on_a_missing_or_unowned_group_return_not_found() {
    let (app, cookie) = common::authed_app().await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/groups/does-not-exist")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    let update_body = serde_json::json!({ "name": "X", "avatar_url": null, "activation_strategy": null }).to_string();
    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/groups/does-not-exist")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/groups/does-not-exist")
                .header(header::COOKIE, cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);

    let members_body = serde_json::json!({ "members": [] }).to_string();
    let members_response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/groups/does-not-exist/members")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, cookie)
                .body(Body::from(members_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(members_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn operations_on_a_group_owned_by_another_user_return_not_found() {
    let (app, first_user_cookie, second_user_cookie) = common::authed_app_with_second_user().await;

    // second user creates a group that the first user has no claim to.
    let create_body = serde_json::json!({
        "name": "Not Yours",
        "avatar_url": null,
        "activation_strategy": "natural"
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, second_user_cookie)
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    // the first user, authenticated but not the owner, should see the same
    // 404 a nonexistent group would give them, not 200 or 403.
    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    let update_body = serde_json::json!({ "name": "Hijacked", "avatar_url": null, "activation_strategy": null }).to_string();
    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::from(update_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NOT_FOUND);

    let members_body = serde_json::json!({ "members": [] }).to_string();
    let members_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}/members"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::from(members_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(members_response.status(), StatusCode::NOT_FOUND);

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, first_user_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn set_members_rejects_a_character_id_belonging_to_another_user() {
    let (app, first_user_cookie, second_user_cookie) = common::authed_app_with_second_user().await;

    let first_user_character_id = create_character(&app, &first_user_cookie, "Aria").await;
    let second_user_character_id = create_character(&app, &second_user_cookie, "Not Yours").await;

    let create_body = serde_json::json!({ "name": "Duo", "avatar_url": null, "activation_strategy": null }).to_string();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/groups")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::from(create_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(created.into_body(), usize::MAX).await.unwrap();
    let group_id = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["id"].as_str().unwrap().to_string();

    // seed the group with a member the first user actually owns, so we can
    // check below that the rejected request didn't touch it.
    let seed_body = serde_json::json!({
        "members": [{ "character_id": first_user_character_id, "disabled": false }]
    })
    .to_string();
    let seeded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}/members"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::from(seed_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(seeded.status(), StatusCode::OK);

    // first user tries to slot the second user's character into their own
    // group. should be rejected as if the character_id didn't exist.
    let malicious_body = serde_json::json!({
        "members": [{ "character_id": second_user_character_id, "disabled": false }]
    })
    .to_string();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/groups/{group_id}/members"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, first_user_cookie.clone())
                .body(Body::from(malicious_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // the group's membership must be exactly what it was before the rejected call.
    let fetched = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/groups/{group_id}"))
                .header(header::COOKIE, first_user_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(fetched.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let members = json["members"].as_array().unwrap();
    assert_eq!(members.len(), 1, "the rejected request must not have changed membership");
    assert_eq!(members[0]["character_id"], first_user_character_id);
}
