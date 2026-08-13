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
}
