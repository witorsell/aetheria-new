use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Group {
    pub user_id: i64,
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub activation_strategy: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct GroupMember {
    pub character_id: String,
    pub position: i64,
    pub disabled: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GroupWithMembers {
    #[serde(flatten)]
    pub group: Group,
    pub members: Vec<GroupMember>,
}

#[derive(Deserialize)]
pub struct GroupInput {
    pub name: String,
    pub avatar_url: Option<String>,
    pub activation_strategy: Option<String>,
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<Group>> {
    sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE user_id = ? ORDER BY name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
}

pub async fn get(pool: &sqlx::SqlitePool, user_id: i64, id: &str) -> sqlx::Result<Option<Group>> {
    sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

pub async fn list_members(pool: &sqlx::SqlitePool, group_id: &str) -> sqlx::Result<Vec<GroupMember>> {
    sqlx::query_as::<_, GroupMember>(
        "SELECT character_id, position, disabled FROM group_members WHERE group_id = ? ORDER BY position ASC",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
}
