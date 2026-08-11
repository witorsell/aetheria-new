use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow)]
struct RegexScriptRow {
    id: String,
    user_id: i64,
    script_name: String,
    find_regex: String,
    replace_string: String,
    trim_strings_json: String,
    placement_json: String,
    disabled: bool,
    markdown_only: bool,
    prompt_only: bool,
    run_on_edit: bool,
    substitute_regex: i32,
    min_depth: Option<i32>,
    max_depth: Option<i32>,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegexScript {
    pub id: String,
    pub user_id: i64,
    pub script_name: String,
    pub find_regex: String,
    pub replace_string: String,
    pub trim_strings: Vec<String>,
    pub placement: Vec<i32>,
    pub disabled: bool,
    pub markdown_only: bool,
    pub prompt_only: bool,
    pub run_on_edit: bool,
    pub substitute_regex: i32,
    pub min_depth: Option<i32>,
    pub max_depth: Option<i32>,
    pub created_at: i64,
}

impl From<RegexScriptRow> for RegexScript {
    fn from(row: RegexScriptRow) -> Self {
        RegexScript {
            id: row.id,
            user_id: row.user_id,
            script_name: row.script_name,
            find_regex: row.find_regex,
            replace_string: row.replace_string,
            trim_strings: serde_json::from_str(&row.trim_strings_json).unwrap_or_default(),
            placement: serde_json::from_str(&row.placement_json).unwrap_or_default(),
            disabled: row.disabled,
            markdown_only: row.markdown_only,
            prompt_only: row.prompt_only,
            run_on_edit: row.run_on_edit,
            substitute_regex: row.substitute_regex,
            min_depth: row.min_depth,
            max_depth: row.max_depth,
            created_at: row.created_at,
        }
    }
}

/// what clients send to create a script, whether pasted by hand or imported
/// straight from a SillyTavern regex script export. also doubles as the
/// export shape (`Serialize`) since it's already exactly SillyTavern's own
/// field names/casing, so a script round-trips through export/import here
/// and stays re-importable into real SillyTavern too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexScriptInput {
    #[serde(rename = "scriptName")]
    pub script_name: String,
    #[serde(rename = "findRegex")]
    pub find_regex: String,
    #[serde(rename = "replaceString", default)]
    pub replace_string: String,
    #[serde(rename = "trimStrings", default)]
    pub trim_strings: Vec<String>,
    #[serde(default)]
    pub placement: Vec<i32>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(rename = "markdownOnly", default)]
    pub markdown_only: bool,
    #[serde(rename = "promptOnly", default)]
    pub prompt_only: bool,
    #[serde(rename = "runOnEdit", default)]
    pub run_on_edit: bool,
    #[serde(rename = "substituteRegex", default)]
    pub substitute_regex: i32,
    #[serde(rename = "minDepth", default)]
    pub min_depth: Option<i32>,
    #[serde(rename = "maxDepth", default)]
    pub max_depth: Option<i32>,
}

impl From<RegexScript> for RegexScriptInput {
    fn from(script: RegexScript) -> Self {
        RegexScriptInput {
            script_name: script.script_name,
            find_regex: script.find_regex,
            replace_string: script.replace_string,
            trim_strings: script.trim_strings,
            placement: script.placement,
            disabled: script.disabled,
            markdown_only: script.markdown_only,
            prompt_only: script.prompt_only,
            run_on_edit: script.run_on_edit,
            substitute_regex: script.substitute_regex,
            min_depth: script.min_depth,
            max_depth: script.max_depth,
        }
    }
}

pub async fn list(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<RegexScript>> {
    sqlx::query_as::<_, RegexScriptRow>("SELECT * FROM regex_scripts WHERE user_id = ? ORDER BY script_name ASC")
        .bind(user_id)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(RegexScript::from).collect())
}

/// the subset actually used to shape prompts sent to the LLM: enabled and
/// flagged `prompt_only`. `markdown_only` scripts only affect how a message
/// is displayed and aren't applied here.
pub async fn list_prompt_only(pool: &sqlx::SqlitePool, user_id: i64) -> sqlx::Result<Vec<RegexScript>> {
    sqlx::query_as::<_, RegexScriptRow>(
        "SELECT * FROM regex_scripts WHERE user_id = ? AND disabled = 0 AND prompt_only = 1 ORDER BY script_name ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(RegexScript::from).collect())
}
