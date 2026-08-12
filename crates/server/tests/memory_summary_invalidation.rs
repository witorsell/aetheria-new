async fn create_user_and_chat(db: &server::db::Db) -> (i64, String) {
    db.writer.create_user("tester".to_string(), "hash".to_string()).await.unwrap();
    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = 'tester'")
        .fetch_one(&db.read_pool)
        .await
        .unwrap();

    let character = db.writer.create_character(user_id, server::models::character::CharacterInput {
        user_id: None,
        name: "Test Character".to_string(),
        description: None,
        personality: None,
        scenario: None,
        first_message: None,
        avatar_url: None,
        sample_chat: None,
        system_prompt: None,
        post_history_instructions: None,
        prefill: None,
        insert_depth_prompt: None,
        insert_depth: None,
        talkativeness: None,
        persona: None,
        extensions: None,
        folder_id: None,
    }).await.unwrap();

    let chat = db.writer.create_chat(user_id, Some(character.id), None, "Test chat".to_string()).await.unwrap();
    (user_id, chat.id)
}

async fn read_summary(db: &server::db::Db, user_id: i64, chat_id: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT memory_summary, memory_summary_message_id FROM chats WHERE id = ? AND user_id = ?",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_one(&db.read_pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn deleting_a_message_already_folded_into_the_summary_invalidates_it() {
    let db = server::db::connect(":memory:").await;
    let (user_id, chat_id) = create_user_and_chat(&db).await;

    let cursor_msg = db.writer.create_message(
        user_id, chat_id.clone(), None, "assistant".to_string(), "the story so far".to_string(),
    ).await.unwrap();

    db.writer.update_chat_memory(
        user_id, chat_id.clone(), "Summary covering up to the cursor.".to_string(), cursor_msg.id.clone(),
    ).await.unwrap();

    let (summary, cursor) = read_summary(&db, user_id, &chat_id).await;
    assert!(summary.is_some(), "summary should be set before the delete");
    assert_eq!(cursor.as_deref(), Some(cursor_msg.id.as_str()));

    // deleting the exact cursor message: it was necessarily already folded
    // in, so the summary can no longer be trusted and must be invalidated
    let deleted = db.writer.soft_delete_message(user_id, cursor_msg.id.clone()).await.unwrap();
    assert!(deleted);

    let (summary, cursor) = read_summary(&db, user_id, &chat_id).await;
    assert!(summary.is_none(), "summary must be cleared once the message it was built from is deleted");
    assert!(cursor.is_none(), "cursor must be cleared alongside the summary");
}

#[tokio::test]
async fn deleting_a_message_before_the_cursor_invalidates_the_summary() {
    let db = server::db::connect(":memory:").await;
    let (user_id, chat_id) = create_user_and_chat(&db).await;

    let earlier_msg = db.writer.create_message(
        user_id, chat_id.clone(), None, "user".to_string(), "an earlier message".to_string(),
    ).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let cursor_msg = db.writer.create_message(
        user_id, chat_id.clone(), Some(earlier_msg.id.clone()), "assistant".to_string(), "later, summarized reply".to_string(),
    ).await.unwrap();

    db.writer.update_chat_memory(
        user_id, chat_id.clone(), "Summary covering both messages.".to_string(), cursor_msg.id.clone(),
    ).await.unwrap();

    let deleted = db.writer.hard_delete_message(user_id, earlier_msg.id.clone()).await.unwrap();
    assert!(deleted);

    let (summary, cursor) = read_summary(&db, user_id, &chat_id).await;
    assert!(summary.is_none(), "deleting content that predates the cursor must still invalidate the summary");
    assert!(cursor.is_none());
}

#[tokio::test]
async fn deleting_a_message_after_the_cursor_leaves_the_summary_intact() {
    let db = server::db::connect(":memory:").await;
    let (user_id, chat_id) = create_user_and_chat(&db).await;

    let cursor_msg = db.writer.create_message(
        user_id, chat_id.clone(), None, "assistant".to_string(), "summarized reply".to_string(),
    ).await.unwrap();
    db.writer.update_chat_memory(
        user_id, chat_id.clone(), "Summary covering the first reply.".to_string(), cursor_msg.id.clone(),
    ).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let refusal = db.writer.create_message(
        user_id, chat_id.clone(), Some(cursor_msg.id.clone()), "assistant".to_string(), "I can't continue this.".to_string(),
    ).await.unwrap();

    // this later message was never folded into the summary (it postdates
    // the cursor), so deleting it - e.g. discarding a refusal to regenerate -
    // must not touch a summary that's still perfectly valid
    let deleted = db.writer.soft_delete_message(user_id, refusal.id.clone()).await.unwrap();
    assert!(deleted);

    let (summary, cursor) = read_summary(&db, user_id, &chat_id).await;
    assert!(summary.is_some(), "summary must survive deleting content that was never part of it");
    assert_eq!(cursor.as_deref(), Some(cursor_msg.id.as_str()));
}

#[tokio::test]
async fn deleting_a_message_in_a_chat_with_no_summary_yet_is_a_no_op() {
    let db = server::db::connect(":memory:").await;
    let (user_id, chat_id) = create_user_and_chat(&db).await;

    let msg = db.writer.create_message(
        user_id, chat_id.clone(), None, "user".to_string(), "hi".to_string(),
    ).await.unwrap();

    let deleted = db.writer.soft_delete_message(user_id, msg.id.clone()).await.unwrap();
    assert!(deleted);

    let (summary, cursor) = read_summary(&db, user_id, &chat_id).await;
    assert!(summary.is_none());
    assert!(cursor.is_none());
}
