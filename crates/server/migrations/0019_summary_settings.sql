ALTER TABLE settings ADD COLUMN summary_provider_type TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN summary_api_base_url TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN summary_api_key TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN summary_model_name TEXT NOT NULL DEFAULT '';
ALTER TABLE settings ADD COLUMN summary_context_limit INTEGER;
