mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use server::db::Db;
use tower::ServiceExt;

async fn character(db: &Db, user_id: i64, name: &str) -> server::models::character::Character {
    let input = serde_json::from_value(serde_json::json!({"name": name})).unwrap();
    db.writer.create_character(user_id, input).await.unwrap()
}

async fn database() -> Db {
    let db = server::db::connect(":memory:").await;
    db.writer.create_user("second-user".into(), "unused".into()).await.unwrap();
    db
}

#[tokio::test]
async fn foreign_character_and_parent_rejections_leave_no_rows() {
    let db = database().await;
    let private = character(&db, 1, "private").await;
    assert!(matches!(db.writer.create_chat(2, Some(private.id.clone()), None, "bad".into()).await,
        Err(sqlx::Error::RowNotFound)));
    let first = db.writer.create_chat(1, Some(private.id.clone()), None, "first".into()).await.unwrap();
    let second = db.writer.create_chat(1, Some(private.id.clone()), None, "second".into()).await.unwrap();
    let parent = db.writer.create_message(1, first.id.clone(), None, "user".into(), "private".into()).await.unwrap();
    for (user_id, chat_id, parent_id) in [
        (2, first.id.clone(), None),
        (1, second.id.clone(), Some(parent.id.clone())),
    ] {
        assert!(matches!(db.writer.create_message(user_id, chat_id, parent_id, "user".into(), "bad".into()).await,
            Err(sqlx::Error::RowNotFound)));
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages").fetch_one(&db.read_pool).await.unwrap();
    assert_eq!(count, 1);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chats").fetch_one(&db.read_pool).await.unwrap();
    assert_eq!(count, 2);

    // legacy invalid references must not expose the character on reads either
    sqlx::query("INSERT INTO chats (id, user_id, character_id, title, created_at, updated_at) VALUES ('legacy', 2, ?, 'legacy', 0, 0)")
        .bind(&private.id).execute(&db.read_pool).await.unwrap();
    let tree = server::models::message::tree_for_chat(&db.read_pool, 2, "legacy").await.unwrap();
    assert!(tree.character.is_none());
}

#[tokio::test]
async fn upload_ownership_does_not_come_from_an_editable_url() {
    let db = database().await;
    let private = character(&db, 1, "private").await;
    let owned = character(&db, 2, "owned").await;
    let filename = format!("avatar_{}.png", private.id);
    assert!(server::uploads::is_owned(&db.read_pool, 1, &filename).await.unwrap());
    assert!(!server::uploads::is_owned(&db.read_pool, 2, &filename).await.unwrap());
    assert!(!server::uploads::is_owned(&db.read_pool, 2, "avatar_user_1.png").await.unwrap());
    assert!(!server::uploads::is_owned(&db.read_pool, 2, "../avatar_user_2.png").await.unwrap());
    db.writer.update_character_avatar(2, owned.id, Some(format!("/uploads/{filename}?v=1"))).await.unwrap();
    let cleanup = db.writer.delete_all_account_content(2).await.unwrap();
    assert!(cleanup.is_empty());
    assert!(server::models::character::get(&db.read_pool, 1, &private.id).await.unwrap().is_some());
}

#[tokio::test]
async fn local_upload_reads_require_the_owner_and_keep_range_support() {
    let (app, owner, other) = common::authed_app_with_second_user().await;
    let response = app.clone().oneshot(Request::builder().method("POST").uri("/api/characters")
        .header(header::COOKIE, owner.as_str()).header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"name":"private avatar"}"#)).unwrap()).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 100_000).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let id = value["id"].as_str().unwrap();
    let (content_type, body) = common::multipart_body("file", "avatar.png", "image/png", b"image-bytes");
    let response = app.clone().oneshot(Request::builder().method("POST").uri(format!("/api/characters/{id}/avatar"))
        .header(header::COOKIE, owner.as_str()).header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let path = format!("/uploads/avatar_{id}.png");
    for cookie in [None, Some(other.as_str())] {
        let mut request = Request::builder().uri(path.as_str());
        if let Some(cookie) = cookie { request = request.header(header::COOKIE, cookie); }
        let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), if cookie.is_some() { StatusCode::NOT_FOUND } else { StatusCode::UNAUTHORIZED });
    }
    let response = app.oneshot(Request::builder().uri(path.as_str()).header(header::COOKIE, owner.as_str())
        .header(header::RANGE, "bytes=0-4").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 100).await.unwrap();
    assert_eq!(&bytes[..], b"image");
    tokio::fs::remove_file(server::auth::uploads_dir().join(format!("avatar_{id}.png"))).await.unwrap();
}

#[tokio::test]
async fn chat_member_edits_do_not_change_another_chats_roster() {
    let db = database().await;
    let a = character(&db, 1, "a").await;
    let b = character(&db, 1, "b").await;
    let c = character(&db, 1, "c").await;
    let group = db.writer.create_group(1, serde_json::from_value(serde_json::json!({"name":"shared"})).unwrap()).await.unwrap();
    db.writer.set_group_members(1, group.id.clone(), vec![(a.id.clone(), false), (b.id.clone(), false)]).await.unwrap();
    let first = db.writer.create_chat(1, None, Some(group.id.clone()), "first".into()).await.unwrap();
    let second = db.writer.create_chat(1, None, Some(group.id.clone()), "second".into()).await.unwrap();
    let third = db.writer.create_chat(1, None, Some(group.id.clone()), "third".into()).await.unwrap();
    db.writer.change_chat_member(1, first.id.clone(), c.id.clone(), true).await.unwrap();
    let updated = server::models::chat::get(&db.read_pool, 1, &first.id).await.unwrap().unwrap();
    assert_ne!(updated.group_id, Some(group.id.clone()));
    let members = server::models::group::list_members(&db.read_pool, &group.id).await.unwrap();
    assert_eq!(members.len(), 2);
    db.writer.change_chat_member(1, second.id.clone(), b.id.clone(), false).await.unwrap();
    let direct = server::models::chat::get(&db.read_pool, 1, &second.id).await.unwrap().unwrap();
    assert_eq!(direct.character_id, Some(a.id));
    assert!(direct.group_id.is_none());
    let unchanged = server::models::chat::get(&db.read_pool, 1, &third.id).await.unwrap().unwrap();
    assert_eq!(unchanged.group_id, Some(group.id.clone()));
    assert_eq!(server::models::group::list_members(&db.read_pool, &group.id).await.unwrap().len(), 2);
    assert!(server::models::chat::get(&db.read_pool, 1, &first.id).await.unwrap().unwrap().group_id.is_some());
}

#[tokio::test]
async fn edits_and_continuations_reject_stale_embedding_results() {
    let db = database().await;
    let character = character(&db, 1, "a").await;
    let chat = db.writer.create_chat(1, Some(character.id), None, "chat".into()).await.unwrap();
    let message = db.writer.create_message(1, chat.id.clone(), None, "assistant".into(), "old".into()).await.unwrap();
    db.writer.insert_memory_chunk(1, chat.id.clone(), message.id.clone(), "assistant".into(), "old".into(), "old".into(), vec![0;4]).await.unwrap();
    db.writer.update_message_content(1, message.id.clone(), "new".into()).await.unwrap();
    db.writer.insert_memory_chunk(1, chat.id.clone(), message.id.clone(), "assistant".into(), "old".into(), "old".into(), vec![0;4]).await.unwrap();
    assert!(server::models::memory_chunk::list_for_chat(&db.read_pool, 1, &chat.id).await.unwrap().is_empty());
    db.writer.insert_memory_chunk(1, chat.id.clone(), message.id.clone(), "assistant".into(), "new".into(), "new".into(), vec![0;4]).await.unwrap();
    assert_eq!(server::models::memory_chunk::list_for_chat(&db.read_pool, 1, &chat.id).await.unwrap()[0].text, "new");
    db.writer.continue_message(1, message.id.clone(), " continued".into(), "[]".into(), 0, 0).await.unwrap();
    db.writer.insert_memory_chunk(1, chat.id.clone(), message.id.clone(), "assistant".into(), "new".into(), "new".into(), vec![0;4]).await.unwrap();
    assert!(server::models::memory_chunk::list_for_chat(&db.read_pool, 1, &chat.id).await.unwrap().is_empty());
}
