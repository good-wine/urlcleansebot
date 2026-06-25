-- Performance indexes for common query patterns

CREATE INDEX IF NOT EXISTS idx_cleaned_links_user_id
    ON cleaned_links(user_id);

CREATE INDEX IF NOT EXISTS idx_cleaned_links_timestamp
    ON cleaned_links(timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_cleaned_links_user_timestamp
    ON cleaned_links(user_id, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_cleaned_links_original_url
    ON cleaned_links(original_url);

CREATE INDEX IF NOT EXISTS idx_custom_rules_user_id
    ON custom_rules(user_id);

CREATE INDEX IF NOT EXISTS idx_whitelist_urls_user_id
    ON whitelist_urls(user_id);
