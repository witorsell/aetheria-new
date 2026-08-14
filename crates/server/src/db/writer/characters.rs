use super::*;
use sqlx::Row;

impl Writer {
    pub async fn create_character(
        &self, user_id: i64,
        input: crate::models::character::CharacterInput,
    ) -> sqlx::Result<crate::models::character::Character> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO characters (user_id, id, name, description, personality, scenario, first_message, \
                 avatar_url, sample_chat, system_prompt, post_history_instructions, prefill, \
                 insert_depth_prompt, insert_depth, talkativeness, persona, extensions, folder_id, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&input.name)
            .bind(input.description.as_deref().unwrap_or(""))
            .bind(input.personality.as_deref().unwrap_or(""))
            .bind(input.scenario.as_deref().unwrap_or(""))
            .bind(input.first_message.as_deref().unwrap_or(""))
            .bind(input.avatar_url.as_deref())
            .bind(input.sample_chat.as_deref().unwrap_or(""))
            .bind(input.system_prompt.as_deref().unwrap_or(""))
            .bind(input.post_history_instructions.as_deref().unwrap_or(""))
            .bind(input.prefill.as_deref().unwrap_or(""))
            .bind(input.insert_depth_prompt.as_deref().unwrap_or(""))
            .bind(input.insert_depth.unwrap_or(3))
            .bind(input.talkativeness.unwrap_or(0.5))
            .bind(input.persona.as_deref().unwrap_or("{}"))
            .bind(input.extensions.as_deref().unwrap_or("{}"))
            .bind(input.folder_id.as_deref())
            .bind(now)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::character::Character {
                user_id,
                insert_depth: input.insert_depth.unwrap_or(3),
                talkativeness: input.talkativeness.unwrap_or(0.5),
                id,
                name: input.name,
                description: input.description.unwrap_or_default(),
                personality: input.personality.unwrap_or_default(),
                scenario: input.scenario.unwrap_or_default(),
                first_message: input.first_message.unwrap_or_default(),
                avatar_path: None,
                avatar_url: input.avatar_url,
                sample_chat: input.sample_chat.unwrap_or_default(),
                system_prompt: input.system_prompt.unwrap_or_default(),
                post_history_instructions: input.post_history_instructions.unwrap_or_default(),
                prefill: input.prefill.unwrap_or_default(),
                insert_depth_prompt: input.insert_depth_prompt.unwrap_or_default(),
                persona: input.persona.unwrap_or_else(|| "{}".into()),
                extensions: input.extensions.unwrap_or_else(|| "{}".into()),
                folder_id: input.folder_id,
                created_at: now,
                updated_at: now,
            })
        })).await
    }

    pub async fn update_character(
        &self, user_id: i64,
        id: String,
        input: crate::models::character::CharacterInput,
    ) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let now = chrono_now_millis();
            sqlx::query(
                "UPDATE characters SET \
                 name = ?, description = ?, personality = ?, scenario = ?, first_message = ?, \
                 avatar_url = ?, sample_chat = ?, system_prompt = ?, post_history_instructions = ?, \
                 prefill = ?, insert_depth_prompt = ?, insert_depth = ?, talkativeness = ?, persona = ?, extensions = ?, folder_id = ?, \
                 updated_at = ? WHERE id = ? AND user_id = ?",
            )
            .bind(&input.name)
            .bind(input.description.as_deref().unwrap_or(""))
            .bind(input.personality.as_deref().unwrap_or(""))
            .bind(input.scenario.as_deref().unwrap_or(""))
            .bind(input.first_message.as_deref().unwrap_or(""))
            .bind(input.avatar_url.as_deref())
            .bind(input.sample_chat.as_deref().unwrap_or(""))
            .bind(input.system_prompt.as_deref().unwrap_or(""))
            .bind(input.post_history_instructions.as_deref().unwrap_or(""))
            .bind(input.prefill.as_deref().unwrap_or(""))
            .bind(input.insert_depth_prompt.as_deref().unwrap_or(""))
            .bind(input.insert_depth.unwrap_or(3))
            .bind(input.talkativeness.unwrap_or(0.5))
            .bind(input.persona.as_deref().unwrap_or("{}"))
            .bind(input.extensions.as_deref().unwrap_or("{}"))
            .bind(input.folder_id.as_deref())
            .bind(now)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|result| result.rows_affected() > 0)
        })).await
    }

    pub async fn update_character_avatar(&self, user_id: i64, id: String, avatar_url: Option<String>) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE characters SET avatar_url = ? WHERE id = ? AND user_id = ?"
            )
            .bind(&avatar_url)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn delete_character(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            let owned = sqlx::query("SELECT 1 FROM characters WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
            if !owned {
                return Ok(false);
            }
            sqlx::query(
                "DELETE FROM messages WHERE chat_id IN (SELECT id FROM chats WHERE character_id = ?)",
            )
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM chats WHERE character_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE messages SET character_id = NULL WHERE character_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM group_members WHERE character_id = ?")
                .bind(&id)
                .execute(&mut *tx)
                .await?;
            let deleted = sqlx::query("DELETE FROM characters WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;
            tx.commit().await?;
            Ok(deleted)
        })).await
    }

    pub async fn create_alternate_greeting(
        &self, user_id: i64,
        character_id: String,
        input: crate::models::character::AlternateGreetingInput,
    ) -> sqlx::Result<crate::models::character::AlternateGreeting> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let res = sqlx::query("INSERT INTO alternate_greetings (user_id, id, character_id, greeting, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?) RETURNING user_id, id, character_id, greeting, created_at, updated_at")
                .bind(user_id)
                .bind(&id)
                .bind(&character_id)
                .bind(&input.greeting)
                .bind(now)
                .bind(now)
                .fetch_one(&mut *conn)
                .await;
            res.map(|r| crate::models::character::AlternateGreeting {
                user_id,
                id: sqlx::Row::try_get(&r, "id").unwrap_or_default(),
                character_id: sqlx::Row::try_get(&r, "character_id").unwrap_or_default(),
                greeting: sqlx::Row::try_get(&r, "greeting").unwrap_or_default(),
                created_at: sqlx::Row::try_get(&r, "created_at").unwrap_or_default(),
            })
        })).await
    }

    pub async fn update_alternate_greeting(&self, user_id: i64, character_id: String, id: String, input: crate::models::character::AlternateGreetingInput) -> sqlx::Result<bool> {
        let greeting = input.greeting;
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE alternate_greetings SET greeting = ? WHERE id = ? AND character_id = ? AND user_id = ?")
                .bind(greeting)
                .bind(id)
                .bind(character_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn delete_alternate_greeting(&self, user_id: i64, character_id: String, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM alternate_greetings WHERE id = ? AND character_id = ? AND user_id = ?")
                .bind(&id)
                .bind(character_id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn create_tag(
        &self, user_id: i64,
        input: crate::models::character::TagInput,
    ) -> sqlx::Result<crate::models::character::Tag> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let color = input.color.as_deref().unwrap_or("#888888");
            // tags carries a UNIQUE(user_id, name) constraint - INSERT OR IGNORE plus
            // a follow-up lookup reuses this user's existing tag of that name instead
            // of erroring, the same collision handling account.rs's import already uses.
            sqlx::query(
                "INSERT OR IGNORE INTO tags (user_id, id, name, color, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&input.name)
            .bind(color)
            .bind(now)
            .execute(&mut *conn)
            .await?;
            sqlx::query_as::<_, crate::models::character::Tag>(
                "SELECT * FROM tags WHERE user_id = ? AND name = ?",
            )
            .bind(user_id)
            .bind(&input.name)
            .fetch_one(&mut *conn)
            .await
        })).await
    }

    pub async fn delete_tag(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM tags WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn set_character_tags(
        &self, user_id: i64,
        character_id: String,
        tag_ids: Vec<String>,
    ) -> sqlx::Result<()> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            sqlx::query("DELETE FROM character_tags WHERE character_id = ? AND user_id = ?")
                .bind(&character_id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            for tag_id in &tag_ids {
                sqlx::query(
                    "INSERT OR IGNORE INTO character_tags (user_id, character_id, tag_id) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(&character_id)
                .bind(tag_id)
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await
        })).await
    }

    pub async fn create_folder(
        &self, user_id: i64,
        input: crate::models::character::FolderInput,
    ) -> sqlx::Result<crate::models::character::Folder> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            sqlx::query(
                "INSERT INTO folders (user_id, id, name, parent_id, created_at) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(user_id)
            .bind(&id)
            .bind(&input.name)
            .bind(&input.parent_id)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::character::Folder {
                user_id,
                id,
                name: input.name,
                parent_id: input.parent_id,
                created_at: now,
            })
        })).await
    }

    pub async fn update_folder(
        &self, user_id: i64,
        id: String,
        input: crate::models::character::FolderInput,
    ) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query(
                "UPDATE folders SET name = ?, parent_id = ? WHERE id = ? AND user_id = ?",
            )
            .bind(&input.name)
            .bind(&input.parent_id)
            .bind(&id)
            .bind(user_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected() > 0)
        })).await
    }

    pub async fn delete_folder(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            let mut tx = conn.begin().await?;
            sqlx::query("UPDATE characters SET folder_id = NULL WHERE folder_id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            let row = sqlx::query("SELECT parent_id FROM folders WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(row) = &row {
                let parent_id: Option<String> = row.get("parent_id");
                sqlx::query("UPDATE folders SET parent_id = ? WHERE parent_id = ? AND user_id = ?")
                    .bind(&parent_id)
                    .bind(&id)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await?;
            }
            let deleted = sqlx::query("DELETE FROM folders WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                > 0;
            tx.commit().await?;
            Ok(deleted)
        })).await
    }
}
