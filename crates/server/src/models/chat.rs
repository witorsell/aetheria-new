use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Chat {
    pub user_id: i64,
    pub id: String,
    pub character_id: Option<String>,
    pub group_id: Option<String>,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub memory_summary: Option<String>,
    pub memory_summary_message_id: Option<String>,
}

// a chat shows up here either because it's still this character's direct
// chat, or because it turned into a group chat and this character is one of
// the members, otherwise a converted chat would vanish from every
// character's list the moment it stops being 1:1.
pub async fn list_for_character(pool: &sqlx::SqlitePool, user_id: i64, character_id: &str) -> sqlx::Result<Vec<Chat>> {
    sqlx::query_as::<_, Chat>(
        "SELECT DISTINCT chats.* FROM chats \
         LEFT JOIN group_members ON group_members.group_id = chats.group_id \
         WHERE (chats.character_id = ? OR group_members.character_id = ?) AND chats.user_id = ? \
         ORDER BY chats.updated_at DESC",
    )
    .bind(character_id)
    .bind(character_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Chat>> {
    sqlx::query_as::<_, Chat>("SELECT * FROM chats WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}
