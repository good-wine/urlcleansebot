-- Initial schema for URLCleanseBot
-- This migration is idempotent — uses IF NOT EXISTS throughout

CREATE TABLE IF NOT EXISTS user_configs (
    user_id INTEGER PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1,
    ai_enabled INTEGER NOT NULL DEFAULT 0,
    mode TEXT NOT NULL DEFAULT 'reply',
    ignored_domains TEXT NOT NULL DEFAULT '',
    cleaned_count INTEGER NOT NULL DEFAULT 0,
    privacy_mode INTEGER NOT NULL DEFAULT 0,
    honor_creator INTEGER NOT NULL DEFAULT 0,
    aggressive_mode INTEGER NOT NULL DEFAULT 0,
    dry_run INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS chat_configs (
    chat_id INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    mode TEXT NOT NULL DEFAULT 'reply',
    added_by INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (chat_id)
);

CREATE TABLE IF NOT EXISTS cleaned_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    original_url TEXT NOT NULL,
    cleaned_url TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'unknown',
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS custom_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    pattern TEXT NOT NULL,
    replacement TEXT NOT NULL DEFAULT '',
    is_active INTEGER NOT NULL DEFAULT 1,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (user_id) REFERENCES user_configs(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS whitelist_urls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    domain TEXT NOT NULL,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, domain),
    FOREIGN KEY (user_id) REFERENCES user_configs(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS feature_flags (
    user_id INTEGER NOT NULL,
    flag_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, flag_name),
    FOREIGN KEY (user_id) REFERENCES user_configs(user_id) ON DELETE CASCADE
);
