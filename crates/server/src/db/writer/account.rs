use super::*;

impl Writer {
    /// deletes every piece of account *content* for this user (characters,
    /// chats/messages, groups, lorebooks, presets, regex scripts, themes,
    /// and the now-orphaned memory_chunks) in one transaction. deliberately
    /// leaves `settings`, `sessions`, and the `users` row itself untouched -
    /// this is content deletion, not account deletion. returns the
    /// avatar_url of every deleted character that had a local one, so the
    /// route layer can remove those files from disk (this function has no
    /// filesystem access, it only knows about the database).
    pub async fn delete_all_account_content(&self, user_id: i64) -> sqlx::Result<Vec<String>> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;

            let avatar_urls: Vec<String> = sqlx::query_scalar(
                "SELECT avatar_url FROM characters WHERE user_id = ? AND avatar_url IS NOT NULL",
            )
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await?;

            // children before parents; order matters for correctness even
            // though this app doesn't enable sqlite FK enforcement, so a
            // wrong order wouldn't error here but would leave orphans
            sqlx::query("DELETE FROM memory_chunks WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM messages WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM chat_lorebooks WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query(
                "DELETE FROM group_members WHERE group_id IN (SELECT id FROM groups WHERE user_id = ?)",
            )
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM chats WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM groups WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM character_lorebooks WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM alternate_greetings WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM character_tags WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM lorebook_entries WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM characters WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM lorebooks WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM tags WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM folders WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM presets WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM regex_scripts WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM themes WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;

            tx.commit().await?;
            Ok(avatar_urls)
        })).await
    }

    /// recreates everything in `export` for `user_id`, always as fresh rows
    /// with fresh ids - never matches or overwrites anything by the
    /// export's original id. safe to call against an empty or non-empty
    /// account. one transaction: a malformed export never leaves a
    /// half-imported account behind.
    pub async fn import_all_account_content(
        &self,
        user_id: i64,
        export: crate::models::account::AccountExport,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            let now = chrono_now_millis();

            // folders: dedup by name within this import, create fresh
            let mut folder_ids_by_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for c in &export.characters {
                if let Some(name) = &c.folder_name {
                    if !folder_ids_by_name.contains_key(name) {
                        let id = uuid::Uuid::new_v4().to_string();
                        sqlx::query("INSERT INTO folders (user_id, id, name, parent_id, created_at) VALUES (?, ?, ?, NULL, ?)")
                            .bind(user_id).bind(&id).bind(name).bind(now)
                            .execute(&mut *tx).await?;
                        folder_ids_by_name.insert(name.clone(), id);
                    }
                }
            }

            // tags: dedup by name within this import, create fresh. `tags.name` is
            // scoped UNIQUE(user_id, name) (see migrations/0030), so INSERT OR IGNORE
            // plus a lookup scoped to this same user_id reuses this account's own
            // existing tag of that name rather than erroring or picking up someone
            // else's row of the same name.
            let mut tag_ids_by_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for c in &export.characters {
                for tag_name in &c.tags {
                    if !tag_ids_by_name.contains_key(tag_name) {
                        let id = uuid::Uuid::new_v4().to_string();
                        sqlx::query("INSERT OR IGNORE INTO tags (user_id, id, name, color, created_at) VALUES (?, ?, ?, '', ?)")
                            .bind(user_id).bind(&id).bind(tag_name).bind(now)
                            .execute(&mut *tx).await?;
                        let actual_id: String = sqlx::query_scalar("SELECT id FROM tags WHERE user_id = ? AND name = ?")
                            .bind(user_id)
                            .bind(tag_name)
                            .fetch_one(&mut *tx)
                            .await?;
                        tag_ids_by_name.insert(tag_name.clone(), actual_id);
                    }
                }
            }

            // characters, remapping old export id -> new db id
            let mut character_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for c in &export.characters {
                let new_id = uuid::Uuid::new_v4().to_string();
                let folder_id = c.folder_name.as_ref().and_then(|n| folder_ids_by_name.get(n));
                sqlx::query(
                    "INSERT INTO characters (user_id, id, name, description, personality, scenario, first_message, \
                     avatar_url, sample_chat, system_prompt, post_history_instructions, prefill, \
                     insert_depth_prompt, insert_depth, talkativeness, persona, extensions, folder_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(user_id).bind(&new_id).bind(&c.name).bind(&c.description).bind(&c.personality)
                .bind(&c.scenario).bind(&c.first_message).bind(&c.sample_chat).bind(&c.system_prompt)
                .bind(&c.post_history_instructions).bind(&c.prefill).bind(&c.insert_depth_prompt)
                .bind(c.insert_depth).bind(c.talkativeness).bind(&c.persona).bind(&c.extensions)
                .bind(folder_id).bind(now).bind(now)
                .execute(&mut *tx).await?;

                for greeting in &c.alternate_greetings {
                    let gid = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO alternate_greetings (user_id, id, character_id, greeting, created_at) VALUES (?, ?, ?, ?, ?)")
                        .bind(user_id).bind(&gid).bind(&new_id).bind(greeting).bind(now)
                        .execute(&mut *tx).await?;
                }
                for tag_name in &c.tags {
                    if let Some(tag_id) = tag_ids_by_name.get(tag_name) {
                        sqlx::query("INSERT OR IGNORE INTO character_tags (user_id, character_id, tag_id) VALUES (?, ?, ?)")
                            .bind(user_id).bind(&new_id).bind(tag_id)
                            .execute(&mut *tx).await?;
                    }
                }

                character_id_map.insert(c.id.clone(), new_id);
            }

            // lorebooks, remapping old export id -> new db id, plus entries
            let mut lorebook_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for l in &export.lorebooks {
                let new_id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO lorebooks (user_id, id, name, description, scan_depth, token_budget, recursive_scanning, extensions, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(user_id).bind(&new_id).bind(&l.name).bind(&l.description).bind(l.scan_depth)
                .bind(l.token_budget).bind(l.recursive_scanning).bind(&l.extensions).bind(now).bind(now)
                .execute(&mut *tx).await?;

                for entry in &l.entries {
                    let eid = uuid::Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO lorebook_entries (user_id, id, lorebook_id, name, entry, keywords, priority, weight, enabled, comment, secondary_keys, constant, position, probability, use_probability, selective, selective_logic, exclude_recursion) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    )
                    .bind(user_id).bind(&eid).bind(&new_id).bind(&entry.name).bind(&entry.entry)
                    .bind(&entry.keywords).bind(entry.priority).bind(entry.weight).bind(entry.enabled)
                    .bind(&entry.comment).bind(&entry.secondary_keys).bind(entry.constant).bind(&entry.position)
                    .bind(entry.probability).bind(entry.use_probability).bind(entry.selective)
                    .bind(entry.selective_logic).bind(entry.exclude_recursion)
                    .execute(&mut *tx).await?;
                }

                lorebook_id_map.insert(l.id.clone(), new_id);
            }

            // character_lorebooks: wire each character back up to its
            // lorebook attachments, now that both id maps exist. mirrors the
            // graceful-skip pattern used for group members and chat
            // lorebooks below - an id that isn't in the map (e.g. a
            // hand-edited export) is silently dropped rather than failing
            // the whole import.
            for c in &export.characters {
                let Some(new_char_id) = character_id_map.get(&c.id) else { continue };
                for old_lb_id in &c.lorebook_ids {
                    if let Some(new_lb_id) = lorebook_id_map.get(old_lb_id) {
                        sqlx::query("INSERT INTO character_lorebooks (user_id, character_id, lorebook_id) VALUES (?, ?, ?)")
                            .bind(user_id).bind(new_char_id).bind(new_lb_id)
                            .execute(&mut *tx).await?;
                    }
                }
            }

            // groups, remapping old export id -> new db id, plus members
            // (resolved through character_id_map - a member whose character
            // wasn't in this export, e.g. a partially-corrupted file, is
            // skipped rather than failing the whole import)
            let mut group_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            for g in &export.groups {
                let new_id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO groups (id, user_id, name, avatar_url, activation_strategy, created_at, updated_at) \
                     VALUES (?, ?, ?, NULL, ?, ?, ?)",
                )
                .bind(&new_id).bind(user_id).bind(&g.name).bind(&g.activation_strategy).bind(now).bind(now)
                .execute(&mut *tx).await?;

                for m in &g.members {
                    if let Some(new_char_id) = character_id_map.get(&m.character_id) {
                        sqlx::query("INSERT INTO group_members (group_id, character_id, position, disabled) VALUES (?, ?, ?, ?)")
                            .bind(&new_id).bind(new_char_id).bind(m.position).bind(m.disabled)
                            .execute(&mut *tx).await?;
                    }
                }

                group_id_map.insert(g.id.clone(), new_id);
            }

            // chats, remapping character/group references, then each
            // chat's message tree (parent-before-child, since parent_id
            // self-references messages.id), then chat_lorebooks
            for chat in &export.chats {
                let new_chat_id = uuid::Uuid::new_v4().to_string();
                let new_character_id = chat.character_id.as_ref().and_then(|id| character_id_map.get(id));
                let new_group_id = chat.group_id.as_ref().and_then(|id| group_id_map.get(id));
                sqlx::query(
                    "INSERT INTO chats (user_id, id, character_id, group_id, title, lorebooks_customized, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(user_id).bind(&new_chat_id).bind(new_character_id).bind(new_group_id)
                .bind(&chat.title).bind(chat.lorebooks_customized).bind(now).bind(now)
                .execute(&mut *tx).await?;

                let mut message_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                // a message can only be inserted once its parent has been
                // (parent_id references messages.id), so make repeated
                // passes over whatever's left until nothing more can be
                // placed - simpler than a proper topological sort and the
                // trees here are shallow enough that this is fine
                let mut remaining: Vec<&crate::models::account::MessageExport> = chat.messages.iter().collect();
                while !remaining.is_empty() {
                    let mut placed_any = false;
                    let mut still_remaining = Vec::new();
                    for m in remaining {
                        let parent_new_id = match &m.parent_id {
                            None => None,
                            Some(old_pid) => match message_id_map.get(old_pid) {
                                Some(new_pid) => Some(new_pid.clone()),
                                None => {
                                    still_remaining.push(m);
                                    continue;
                                }
                            },
                        };
                        let new_msg_id = uuid::Uuid::new_v4().to_string();
                        let msg_character_id = m.character_id.as_ref().and_then(|id| character_id_map.get(id));
                        sqlx::query(
                            "INSERT INTO messages (user_id, id, chat_id, parent_id, role, content, visible, deleted, raw_prompt, prompt_tokens, context_limit, character_id, created_at) \
                             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?)",
                        )
                        .bind(user_id).bind(&new_msg_id).bind(&new_chat_id).bind(&parent_new_id)
                        .bind(&m.role).bind(&m.content).bind(m.visible).bind(&m.raw_prompt)
                        .bind(m.prompt_tokens).bind(m.context_limit).bind(msg_character_id).bind(now)
                        .execute(&mut *tx).await?;
                        message_id_map.insert(m.id.clone(), new_msg_id);
                        placed_any = true;
                    }
                    if !placed_any {
                        // a parent_id pointing outside this chat's own
                        // message list (shouldn't happen from a real
                        // export, but a hand-edited file could do it) -
                        // stop rather than loop forever
                        break;
                    }
                    remaining = still_remaining;
                }

                for old_lb_id in &chat.lorebook_ids {
                    if let Some(new_lb_id) = lorebook_id_map.get(old_lb_id) {
                        sqlx::query("INSERT INTO chat_lorebooks (user_id, chat_id, lorebook_id) VALUES (?, ?, ?)")
                            .bind(user_id).bind(&new_chat_id).bind(new_lb_id)
                            .execute(&mut *tx).await?;
                    }
                }
            }

            // presets, regex_scripts, themes - no cross-references to remap
            for p in &export.presets {
                let id = uuid::Uuid::new_v4().to_string();
                let prompts_json = serde_json::to_string(&p.prompts).unwrap_or_else(|_| "[]".to_string());
                let order_json = serde_json::to_string(&p.prompt_order).unwrap_or_else(|_| "[]".to_string());
                sqlx::query("INSERT INTO presets (id, user_id, name, prompts_json, prompt_order_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
                    .bind(&id).bind(user_id).bind(&p.name).bind(&prompts_json).bind(&order_json).bind(now).bind(now)
                    .execute(&mut *tx).await?;
            }

            for r in &export.regex_scripts {
                let id = uuid::Uuid::new_v4().to_string();
                let trim_json = serde_json::to_string(&r.trim_strings).unwrap_or_else(|_| "[]".to_string());
                let placement_json = serde_json::to_string(&r.placement).unwrap_or_else(|_| "[]".to_string());
                sqlx::query(
                    "INSERT INTO regex_scripts (id, user_id, script_name, find_regex, replace_string, trim_strings_json, placement_json, disabled, markdown_only, prompt_only, run_on_edit, substitute_regex, min_depth, max_depth, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id).bind(user_id).bind(&r.script_name).bind(&r.find_regex).bind(&r.replace_string)
                .bind(&trim_json).bind(&placement_json).bind(r.disabled).bind(r.markdown_only)
                .bind(r.prompt_only).bind(r.run_on_edit).bind(r.substitute_regex).bind(r.min_depth)
                .bind(r.max_depth).bind(now)
                .execute(&mut *tx).await?;
            }

            for t in &export.themes {
                let id = uuid::Uuid::new_v4().to_string();
                let tokens_json = serde_json::to_string(&t.tokens).unwrap_or_else(|_| "{}".to_string());
                sqlx::query("INSERT INTO themes (id, user_id, name, token_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
                    .bind(&id).bind(user_id).bind(&t.name).bind(&tokens_json).bind(now).bind(now)
                    .execute(&mut *tx).await?;
            }

            tx.commit().await?;
            Ok(())
        })).await
    }
}
