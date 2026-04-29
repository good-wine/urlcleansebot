# Fixes Applied to ClearURLs Bot

## Recent Fixes

### 1. `db/implementation.rs` — Data loss on restart (SQLite)

**Problem:** `DROP TABLE` + `CREATE TABLE` on every `init()` call destroyed all data on restart.

**Fix:** Replaced with `CREATE TABLE IF NOT EXISTS` for all 7 tables. Data now persists across restarts.

### 2. `db/implementation.rs` — `increment_cleaned_count` silently failed

**Problem:** `UPDATE user_configs SET cleaned_count = ... WHERE user_id = ?` did nothing when the row didn't exist (e.g., when `get_user_config` returned a default without inserting).

**Fix:** Changed to `INSERT ... ON CONFLICT(user_id) DO UPDATE SET cleaned_count = cleaned_count + ?`.

### 3. `db/implementation.rs` — Missing `privacy_mode` column

**Problem:** The `UserConfig` model had a `privacy_mode` field but the schema was missing it, causing "no column found" errors on read.

**Fix:** Added `privacy_mode INTEGER NOT NULL DEFAULT 0` to the `user_configs` table schema and updated `save_user_config` to include it.

### 4. `config.rs` — Duplicated `PORT` parsing with conflicting defaults

**Problem:** `PORT` was parsed twice — once as a `String` with default `"3000"` (used for `SERVER_ADDR`) and once as a `u16` with default `8080`.

**Fix:** Single parse as `u16` with default `8080`, shared by both `port` and `server_addr` construction. Removed `DEFAULT_PORT` string constant.

### 5. Security modules — Duplicated code in `src/security.rs` and `src/shared/security.rs`

**Problem:** Two separate security modules with overlapping functionality (rate limiting, input sanitization, URL validation).

**Fix:** Merged everything into `shared/security.rs` with:
- Sync `RateLimiter` + static `RATE_LIMITER` (for message handlers)
- Async `check_rate_limit` (for query handlers)
- `sanitize_input`, `sanitize_callback`, `sanitize_telegram_text`
- `validate_url`, `validate_user_id`, `validate_domain`, `is_admin`
- `SecurityError` enum
- Deleted `src/security.rs` and `pub mod security;` from `lib.rs`
- Updated imports in `presentation/telegram/handlers.rs`

### 6. `i18n.rs` — 115-line commented-out dead code

**Problem:** `load_translations_from_file()` was entirely commented out with a note about `&'static str` lifetime issues.

**Fix:** Removed the entire commented block.

### 7. `i18n.rs` — Unused `d_ignored_domains` field

**Problem:** `#[allow(dead_code)]` attribute on a field that was never read anywhere in the codebase.

**Fix:** Removed the field from the `Translations` struct and all language translation implementations.

### 8. `domain/services/mod.rs` — Unused skeleton traits

**Problem:** `UrlCleaningService`, `SecurityService`, `FrontendService`, `UserService`, `StatisticsService` traits were defined but never implemented or used.

**Fix:** Deleted the entire `domain/services/` directory and removed `pub mod services;` from `domain/mod.rs`.

### 9. Integration tests — Shared SQLite in-memory database

**Problem:** All tests used `sqlite::memory:` which caused "table already exists" errors when tests ran in parallel.

**Fix:** Each test gets a unique database via `sqlite:file:testdb{id}?mode=memory&cache=shared` with an atomic counter.

### 10. Sanitizer tests — `RuleEngine::new_lazy()` doesn't load rules

**Problem:** Tests called `sanitize()` on an engine with no rules loaded, causing `unwrap()` panics.

**Fix:** Changed to async `RuleEngine::new(...).await` which fetches and loads ClearURLs rules.

---

## Legacy Fixes (from earlier versions)

### `db/implementation.rs` — Non-existent `connect_options()` call

`Pool<Any>` in sqlx 0.8 does not expose `connect_options()`. Fixed by storing the raw URL string in `Db` and deriving backend type via `is_sqlite()` helper.

### `sanitize_input` — Rejected all non-URL text

Original implementation returned empty string for any input not starting with `http://`/`https://`, breaking inline queries. Fixed to only strip control characters and cap length.
