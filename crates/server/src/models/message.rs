use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, sqlx::FromRow, Clone)]
pub struct Message {
    pub user_id: i64,
    pub id: String,
    pub chat_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub visible: bool,
    pub deleted: bool,
    pub created_at: i64,
    // JSON-serialized messages array sent to the provider, only set for
    // live-generated assistant replies
    pub raw_prompt: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub context_limit: Option<i64>,
    pub character_id: Option<String>,
}

impl Message {
    pub fn role_enum(&self) -> crate::provider::prompt::Role {
        crate::provider::prompt::Role::from(self.role.as_str())
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MessageTree {
    pub user_id: i64,
    pub root_id: Option<String>,
    pub messages: HashMap<String, MessageNode>,
    pub character: Option<crate::models::character::Character>,
    pub group: Option<crate::models::group::GroupWithMembers>,
    pub user_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MessageNode {
    pub user_id: i64,
    pub id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub visible: bool,
    pub deleted: bool,
    pub created_at: i64,
    pub children: Vec<String>,
    pub raw_prompt: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub context_limit: Option<i64>,
    pub character_id: Option<String>,
}

pub async fn tree_for_chat(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str) -> sqlx::Result<MessageTree> {
    let rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE chat_id = ? AND user_id = ? AND deleted = 0 ORDER BY created_at ASC",
    )
    .bind(chat_id)
        .bind(user_id)
    .fetch_all(pool)
    .await?;

    let character = sqlx::query_as::<_, crate::models::character::Character>(
        "SELECT characters.* FROM characters JOIN chats ON chats.character_id = characters.id WHERE chats.id = ? AND chats.user_id = ?"
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let chat = crate::models::chat::get(pool, user_id, chat_id).await?;
    let mut group = None;
    if let Some(group_id) = chat.as_ref().and_then(|c| c.group_id.clone()) {
        if let Some(g) = crate::models::group::get(pool, user_id, &group_id).await? {
            let members = crate::models::group::list_members(pool, &group_id).await?;
            group = Some(crate::models::group::GroupWithMembers { group: g, members });
        }
    }

    let mut messages: HashMap<String, MessageNode> = HashMap::new();
    let mut root_id: Option<String> = None;

    for msg in &rows {
        if msg.parent_id.is_none() {
            root_id = Some(msg.id.clone());
        }
        messages.insert(
            msg.id.clone(),
            MessageNode { user_id,
                id: msg.id.clone(),
                parent_id: msg.parent_id.clone(),
                role: msg.role.clone(),
                content: msg.content.clone(),
                visible: msg.visible,
                deleted: msg.deleted,
                created_at: msg.created_at,
                children: Vec::new(),
                raw_prompt: msg.raw_prompt.clone(),
                prompt_tokens: msg.prompt_tokens,
                context_limit: msg.context_limit,
                character_id: msg.character_id.clone(),
            },
        );
    }

    for msg in &rows {
        if let Some(parent_id) = &msg.parent_id {
            if let Some(parent) = messages.get_mut(parent_id) {
                parent.children.push(msg.id.clone());
            }
        }
    }

    let mut user_name = None;
    let user = crate::models::user::find_by_id(pool, user_id).await?;
    if let Some(u) = user {
        user_name = Some(u.display_name.clone().or_else(|| Some(u.username.clone())).unwrap_or_default());
    }

    Ok(MessageTree { user_id,
        root_id,
        messages,
        character,
        group,
        user_name,
    })
}

pub async fn active_branch_for_chat(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str, limit: Option<i64>) -> sqlx::Result<Vec<Message>> {
    let limit_val = limit.unwrap_or(100);
    sqlx::query_as::<_, Message>(
        "WITH RECURSIVE active_branch(id, parent_id, depth) AS (
            SELECT id, parent_id, 0 FROM messages
            WHERE chat_id = ? AND user_id = ? AND parent_id IS NULL AND deleted = 0
            UNION ALL
            SELECT m.id, m.parent_id, ab.depth + 1 FROM messages m
            JOIN active_branch ab ON m.parent_id = ab.id AND m.deleted = 0
            WHERE m.id = (
                SELECT id FROM messages 
                WHERE parent_id = ab.id AND deleted = 0 
                ORDER BY created_at DESC LIMIT 1
            )
        )
        SELECT messages.* FROM messages
        JOIN active_branch USING (id)
        ORDER BY messages.created_at ASC
        LIMIT ?"
    )
    .bind(chat_id)
    .bind(user_id)
    .bind(limit_val)
    .fetch_all(pool)
    .await
}

// root to leaf, following the last (most recent) child at each node
pub fn active_branch(tree: &MessageTree) -> Vec<MessageNode> {
    let mut branch = Vec::new();
    let mut current_id = match &tree.root_id {
        Some(id) => id.clone(),
        None => return branch,
    };

    loop {
        let node = match tree.messages.get(&current_id) {
            Some(n) => n.clone(),
            None => break,
        };
        let has_children = !node.children.is_empty();
        branch.push(node);
        if !has_children {
            break;
        }
        current_id = branch
            .last()
            .expect("just pushed a node above, so branch is non-empty")
            .children
            .last()
            .expect("has_children was just checked true for this node")
            .clone();
    }

    branch
}

// root to target_id, following each node's real parent, root-first
pub fn ancestor_path(tree: &MessageTree, target_id: &str) -> Option<Vec<MessageNode>> {
    let mut path = Vec::new();
    let mut current_id = target_id.to_string();
    loop {
        let node = tree.messages.get(&current_id)?.clone();
        let parent_id = node.parent_id.clone();
        path.push(node);
        match parent_id {
            Some(pid) => current_id = pid,
            None => break,
        }
    }
    path.reverse();
    Some(path)
}

// selected_children maps parent_id -> chosen child_id, defaults to the
// last child when a node has no entry
pub fn branch_path(
    tree: &MessageTree,
    selected_children: &HashMap<String, String>,
) -> Vec<MessageNode> {
    let mut branch = Vec::new();
    let mut current_id = match &tree.root_id {
        Some(id) => id.clone(),
        None => return branch,
    };

    loop {
        let node = match tree.messages.get(&current_id) {
            Some(n) => n.clone(),
            None => break,
        };
        if node.children.is_empty() {
            branch.push(node);
            break;
        }
        branch.push(node.clone());
        // use the user's selection if present, otherwise follow the last child.
        current_id = selected_children
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| node.children.last().unwrap().clone());
    }

    branch
}

pub async fn list_for_chat(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    chat_id: &str,
    before: Option<&str>,
    limit: i64,
) -> sqlx::Result<Vec<Message>> {
    use sqlx::Row;

    let rows = if let Some(before_ts) = before {
        sqlx::query(
            "SELECT * FROM messages WHERE chat_id = ? AND user_id = ? AND deleted = 0 AND created_at < ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(chat_id)
        .bind(user_id)
        .bind(before_ts)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT * FROM messages WHERE chat_id = ? AND user_id = ? AND deleted = 0 ORDER BY created_at DESC LIMIT ?",
        )
        .bind(chat_id)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await?
    };

    let mut messages: Vec<Message> = rows
        .into_iter()
        .map(|row| Message { user_id: row.get("user_id"),
            id: row.get("id"),
            chat_id: row.get("chat_id"),
            parent_id: row.get("parent_id"),
            role: row.get("role"),
            content: row.get("content"),
            visible: row.get("visible"),
            deleted: row.get("deleted"),
            created_at: row.get("created_at"),
            raw_prompt: row.get("raw_prompt"),
            prompt_tokens: row.get("prompt_tokens"),
            context_limit: row.get("context_limit"),
            character_id: row.get("character_id"),
        })
        .collect();

    messages.reverse();
    Ok(messages)
}

/// fetch a subtree rooted at `from_id` up to `depth` generations deep.
/// used for lazy-loading branches in the frontend.
pub async fn tree_from_message(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    chat_id: &str,
    from_id: &str,
    depth: usize,
) -> sqlx::Result<HashMap<String, MessageNode>> {
    use std::collections::VecDeque;

    // first verify the message exists and belongs to this chat/user
    let root = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE id = ? AND chat_id = ? AND user_id = ? AND deleted = 0",
    )
    .bind(from_id)
    .bind(chat_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    let Some(root) = root else {
        return Ok(HashMap::new());
    };

    // BFS to collect descendants up to depth. capped independently of depth
    // since a wide (not just deep) tree can still blow past SQLite's bound
    // parameter limit in the bulk IN (...) fetch below - a deeply-branched
    // chat after months of regenerate/sibling use is a real way to hit this,
    // not just a pathological depth value.
    const MAX_TREE_NODES: usize = 900;
    let mut result: HashMap<String, MessageNode> = HashMap::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((root.id.clone(), 0));

    // we'll need to query for children at each level. to avoid n queries,
    // collect all candidate IDs and do one bulk fetch
    let mut all_ids = vec![root.id.clone()];
    let mut depth_map: HashMap<String, usize> = HashMap::new();
    depth_map.insert(root.id.clone(), 0);

    while let Some((current_id, current_depth)) = queue.pop_front() {
        if current_depth >= depth || all_ids.len() >= MAX_TREE_NODES {
            continue;
        }
        let children = sqlx::query_scalar::<_, String>(
            "SELECT id FROM messages WHERE parent_id = ? AND chat_id = ? AND user_id = ? AND deleted = 0 ORDER BY created_at ASC",
        )
        .bind(&current_id)
        .bind(chat_id)
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for child_id in children {
            if all_ids.len() >= MAX_TREE_NODES {
                break;
            }
            if !depth_map.contains_key(&child_id) {
                depth_map.insert(child_id.clone(), current_depth + 1);
                all_ids.push(child_id.clone());
                queue.push_back((child_id, current_depth + 1));
            }
        }
    }

    // bulk fetch all messages
    if !all_ids.is_empty() {
        let placeholders = all_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT * FROM messages WHERE id IN ({placeholders}) AND chat_id = ? AND user_id = ? AND deleted = 0",
        );
        let mut q = sqlx::query_as::<_, Message>(sqlx::AssertSqlSafe(query));
        for id in &all_ids {
            q = q.bind(id);
        }
        q = q.bind(chat_id).bind(user_id);
        let rows = q.fetch_all(pool).await?;

        // build nodes
        for msg in &rows {
            result.insert(
                msg.id.clone(),
                MessageNode {
                    user_id,
                    id: msg.id.clone(),
                    parent_id: msg.parent_id.clone(),
                    role: msg.role.clone(),
                    content: msg.content.clone(),
                    visible: msg.visible,
                    deleted: msg.deleted,
                    created_at: msg.created_at,
                    children: Vec::new(),
                    raw_prompt: msg.raw_prompt.clone(),
                    prompt_tokens: msg.prompt_tokens,
                    context_limit: msg.context_limit,
                    character_id: msg.character_id.clone(),
                },
            );
        }

        // link children to parents
        for msg in &rows {
            if let Some(parent_id) = &msg.parent_id {
                if let Some(parent) = result.get_mut(parent_id) {
                    parent.children.push(msg.id.clone());
                }
            }
        }
    }

    Ok(result)
}
