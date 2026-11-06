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
        "SELECT characters.* FROM characters JOIN chats ON chats.character_id = characters.id WHERE chats.id = ? AND chats.user_id = ? AND characters.user_id = chats.user_id"
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

/// the history view only needs the text the user reads. `raw_prompt` is the
/// full provider payload for every reply, and sending all of them makes the
/// reload too big to come back promptly.
pub fn strip_raw_prompts(tree: &mut MessageTree) {
    for node in tree.messages.values_mut() {
        node.raw_prompt = None;
    }
}

/// the active branch of a chat, root first, picked with the same rule as
/// `active_branch`. a chat longer than `limit` keeps its newest `limit`
/// messages, not its oldest ones.
pub async fn active_branch_for_chat(pool: &sqlx::SqlitePool, user_id: i64, chat_id: &str, limit: Option<i64>) -> sqlx::Result<Vec<Message>> {
    let limit_val = limit.unwrap_or(100).max(0) as usize;
    let rows = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE chat_id = ? AND user_id = ? AND deleted = 0 ORDER BY created_at ASC",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let rank = rank_subtrees(
        rows.iter()
            .map(|m| (m.id.as_str(), m.parent_id.as_deref(), m.role == "assistant", m.created_at)),
    );
    let mut children: HashMap<&str, Vec<&Message>> = HashMap::new();
    let mut root: Option<&Message> = None;
    for m in &rows {
        match m.parent_id.as_deref() {
            Some(pid) => children.entry(pid).or_default().push(m),
            None => root = Some(m),
        }
    }
    let mut branch: Vec<Message> = Vec::new();
    let mut cur = root;
    while let Some(m) = cur {
        branch.push(m.clone());
        cur = children.get(m.id.as_str()).and_then(|kids| {
            kids.iter()
                .copied()
                .max_by_key(|k| rank.get(k.id.as_str()).copied().unwrap_or((false, 0)))
        });
    }
    if branch.len() > limit_val {
        branch.drain(..branch.len() - limit_val);
    }
    Ok(branch)
}

/// how a subtree ranks when the branch has to be picked without a stored
/// choice: a subtree holding a reply beats one that is only an unanswered
/// line, then the one with the newest message wins. a stray line that never
/// got a reply (or whose reply was deleted) must not outrank the branch that
/// actually holds the conversation, even when it is newer than what is left
/// of that branch after its last messages were deleted.
pub(crate) fn rank_subtrees<'a>(
    items: impl Iterator<Item = (&'a str, Option<&'a str>, bool, i64)>,
) -> HashMap<String, (bool, i64)> {
    let mut rows: Vec<(&str, Option<&str>, bool, i64)> = items.collect();
    let parent_of: HashMap<&str, Option<&str>> = rows.iter().map(|r| (r.0, r.1)).collect();
    rows.sort_by(|a, b| b.3.cmp(&a.3));
    let mut newest: HashMap<&str, i64> = HashMap::new();
    let mut answered: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for &(id, _, is_reply, created_at) in &rows {
        let mut cur = Some(id);
        while let Some(n) = cur {
            if newest.contains_key(n) {
                break;
            }
            newest.insert(n, created_at);
            cur = parent_of.get(n).copied().flatten();
        }
        if is_reply {
            let mut cur = Some(id);
            while let Some(n) = cur {
                if !answered.insert(n) {
                    break;
                }
                cur = parent_of.get(n).copied().flatten();
            }
        }
    }
    newest
        .into_iter()
        .map(|(id, t)| (id.to_string(), (answered.contains(id), t)))
        .collect()
}

fn newest_in_subtree(tree: &MessageTree) -> HashMap<String, (bool, i64)> {
    rank_subtrees(
        tree.messages
            .values()
            .map(|n| (n.id.as_str(), n.parent_id.as_deref(), n.role == "assistant", n.created_at)),
    )
}

// root to leaf, following at each node the child whose subtree holds the
// newest message
pub fn active_branch(tree: &MessageTree) -> Vec<MessageNode> {
    let newest = newest_in_subtree(tree);
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
            .iter()
            .max_by_key(|id| newest.get(id.as_str()).copied().unwrap_or((false, 0)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_raw_prompts_keeps_the_visible_message() {
        let mut tree = MessageTree {
            user_id: 1,
            root_id: Some("a".into()),
            messages: HashMap::new(),
            character: None,
            group: None,
            user_name: None,
        };
        tree.messages.insert(
            "a".into(),
            MessageNode {
                user_id: 1,
                id: "a".into(),
                parent_id: None,
                role: "assistant".into(),
                content: "hi".into(),
                visible: true,
                deleted: false,
                created_at: 0,
                children: Vec::new(),
                raw_prompt: Some("[{\"role\":\"system\"}]".into()),
                prompt_tokens: Some(12),
                context_limit: Some(8000),
                character_id: None,
            },
        );
        strip_raw_prompts(&mut tree);
        let node = &tree.messages["a"];
        assert!(node.raw_prompt.is_none());
        assert_eq!(node.prompt_tokens, Some(12));
        assert_eq!(node.content, "hi");
    }
}

#[cfg(test)]
mod branch_tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>, created_at: i64, children: &[&str]) -> MessageNode {
        MessageNode {
            user_id: 1,
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
            role: "user".to_string(),
            content: String::new(),
            visible: true,
            deleted: false,
            created_at,
            children: children.iter().map(|s| s.to_string()).collect(),
            raw_prompt: None,
            prompt_tokens: None,
            context_limit: None,
            character_id: None,
        }
    }

    fn tree(nodes: Vec<MessageNode>) -> MessageTree {
        let root_id = nodes.iter().find(|n| n.parent_id.is_none()).map(|n| n.id.clone());
        MessageTree {
            user_id: 1,
            root_id,
            messages: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            character: None,
            group: None,
            user_name: None,
        }
    }

    #[test]
    fn a_newer_dead_end_sibling_does_not_hide_the_long_branch() {
        let t = tree(vec![
            node("root", None, 1, &["long", "stray"]),
            node("long", Some("root"), 2, &["a1"]),
            node("a1", Some("long"), 3, &["u2"]),
            node("u2", Some("a1"), 4, &["a2"]),
            node("a2", Some("u2"), 9, &[]),
            node("stray", Some("root"), 6, &[]),
        ]);
        let ids: Vec<_> = active_branch(&t).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["root", "long", "a1", "u2", "a2"]);
    }

    #[test]
    fn a_stray_line_stays_hidden_after_the_branch_tail_is_deleted() {
        // the long branch lost its newest messages, so the unanswered
        // line is now newer than everything left on it
        let mut stray = node("stray", Some("root"), 8, &[]);
        stray.role = "user".to_string();
        let mut a1 = node("a1", Some("long"), 3, &[]);
        a1.role = "assistant".to_string();
        let t = tree(vec![
            node("root", None, 1, &["long", "stray"]),
            node("long", Some("root"), 2, &["a1"]),
            a1,
            stray,
        ]);
        let ids: Vec<_> = active_branch(&t).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["root", "long", "a1"]);
    }

    #[test]
    fn a_newer_regeneration_still_wins() {
        let t = tree(vec![
            node("root", None, 1, &["u"]),
            node("u", Some("root"), 2, &["old", "new"]),
            node("old", Some("u"), 3, &[]),
            node("new", Some("u"), 4, &[]),
        ]);
        let ids: Vec<_> = active_branch(&t).into_iter().map(|n| n.id).collect();
        assert_eq!(ids, vec!["root", "u", "new"]);
    }
}
