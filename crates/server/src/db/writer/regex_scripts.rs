use super::*;

impl Writer {
    pub async fn create_regex_script(
        &self, user_id: i64,
        input: crate::models::regex_script::RegexScriptInput,
    ) -> sqlx::Result<crate::models::regex_script::RegexScript> {
        self.dispatch(move |conn| Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono_now_millis();
            let trim_strings_json = serde_json::to_string(&input.trim_strings).unwrap_or_else(|_| "[]".to_string());
            let placement_json = serde_json::to_string(&input.placement).unwrap_or_else(|_| "[]".to_string());
            sqlx::query(
                "INSERT INTO regex_scripts (id, user_id, script_name, find_regex, replace_string, trim_strings_json, placement_json, disabled, markdown_only, prompt_only, run_on_edit, substitute_regex, min_depth, max_depth, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(user_id)
            .bind(&input.script_name)
            .bind(&input.find_regex)
            .bind(&input.replace_string)
            .bind(&trim_strings_json)
            .bind(&placement_json)
            .bind(input.disabled)
            .bind(input.markdown_only)
            .bind(input.prompt_only)
            .bind(input.run_on_edit)
            .bind(input.substitute_regex)
            .bind(input.min_depth)
            .bind(input.max_depth)
            .bind(now)
            .execute(&mut *conn)
            .await
            .map(|_| crate::models::regex_script::RegexScript {
                id,
                user_id,
                script_name: input.script_name,
                find_regex: input.find_regex,
                replace_string: input.replace_string,
                trim_strings: input.trim_strings,
                placement: input.placement,
                disabled: input.disabled,
                markdown_only: input.markdown_only,
                prompt_only: input.prompt_only,
                run_on_edit: input.run_on_edit,
                substitute_regex: input.substitute_regex,
                min_depth: input.min_depth,
                max_depth: input.max_depth,
                created_at: now,
            })
        })).await
    }

    pub async fn delete_regex_script(&self, user_id: i64, id: String) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("DELETE FROM regex_scripts WHERE id = ? AND user_id = ?")
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }

    pub async fn set_regex_script_disabled(&self, user_id: i64, id: String, disabled: bool) -> sqlx::Result<bool> {
        self.dispatch(move |conn| Box::pin(async move {
            sqlx::query("UPDATE regex_scripts SET disabled = ? WHERE id = ? AND user_id = ?")
                .bind(disabled)
                .bind(&id)
                .bind(user_id)
                .execute(&mut *conn)
                .await
                .map(|res| res.rows_affected() > 0)
        })).await
    }
}
