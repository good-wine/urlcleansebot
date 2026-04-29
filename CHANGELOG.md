# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Internationalization

- **Expanded to 15 languages** — Added Arabic, Hindi, Chinese, Japanese, Korean, Turkish, Dutch, Polish (alongside existing IT, EN, ES, FR, DE, PT, RU)
- **Refactored language selector** — Inline keyboard now displays all 15 languages in a 4-column grid
- **`/setlang` updated** — Accepts all 15 language codes with helpful error messages
- **Settings callbacks updated** — Language validation expanded to all supported codes
- **Fixed Polish translation block** — Corrected corrupted `pl` translations in `i18n.rs`
- **Added `_ =>` fallback** — Defaults to English for unknown language codes
- **Centralized language name mapping** — Extracted into `helpers::language_name()` to eliminate 90+ lines of duplication
- **Auto-detection expanded** — `whatlang` now maps all 15 supported languages from message content
- **New `LANGUAGES.md`** — Complete translation guide and language reference

### Bug Fixes

- **Memory leak fixed** — Removed `Box::leak()` in `handle_message()` that leaked memory on every processed message
- **Port validation bug** — Fixed substring match (`server_addr.contains("8080")` matched `"18080"`) with proper port parsing
- **URL_REGEX bug** — Fixed `.[^\s]*` (literal dot) to `[^\s]*` (any character) in URL validation regex
- **PostgreSQL compatibility** — `get_domain_cleanup_stats()` now branches on database type (`SUBSTR/INSTR` for SQLite, `SPLIT_PART` for PostgreSQL)
- **Logging inconsistency** — Replaced all `log::error!` calls in `config.rs` with `tracing::warn!`

### Code Quality

- **Consolidated sanitization** — Merged duplicate `sanitize_input()` / `sanitize_callback()` into single `sanitize_string()` helper with early-return for clean input
- **Removed dead code** — Deleted unused `Timer` struct in `logging.rs`, unused `_user_config` in `settings.rs`
- **Renamed local constant** — `MAX_MESSAGE_LENGTH` (handlers) → `MAX_RESPONSE_LENGTH` to avoid conflict with `shared/security.rs::MAX_MESSAGE_LENGTH` (4096)
- **Consolidated security modules** — Merged `src/security.rs` and `src/shared/security.rs` into a single `shared/security.rs` with both sync and async rate limiters, input sanitization, and URL validation
- **Removed dead code** — Deleted commented-out `load_translations_from_file()` in `i18n.rs`, unused `d_ignored_domains` field, and unused `domain/services/` skeleton traits
- **Fixed config port duplication** — Single source of truth for `PORT` with unified default (8080)
- **Fixed database initialization** — Replaced `DROP TABLE` + `CREATE TABLE` with `CREATE TABLE IF NOT EXISTS` — SQLite data now persists across restarts
- **Fixed `increment_cleaned_count` upsert** — Changed from `UPDATE` to `INSERT ... ON CONFLICT DO UPDATE`
- **Added missing `privacy_mode` column** — Schema now matches `UserConfig` model

### Testing

- **Fixed integration test isolation** — Tests now use unique `sqlite:file:testdb{id}?mode=memory&cache=shared` URIs instead of shared `sqlite::memory:`
- **Fixed sanitizer tests** — Changed from `RuleEngine::new_lazy()` to `RuleEngine::new().await` to actually load ClearURLs rules
- **90 tests total** — 63 unit + 8 bot commands + 10 database + 9 sanitizer, all passing
- **CI pipeline** — `.github/workflows/ci.yml` with check, clippy, test on push/PR

### Documentation

- **Updated README.md** — Current architecture, 15-language table, LANGUAGES.md link
- **Updated ARCHITECTURE.md** — i18n section, updated test counts (90)
- **Updated docs/ARCHITECTURE.md** — Language table, detection pipeline, test counts
- **Updated CONTRIBUTING.md** — Test counts, language contribution guide
- **Updated QUICK_START.md** — All 15 language codes in `/setlang`
- **Updated docs/DEPLOYMENT.md** — Language section, security updates, Rust 1.88 MSRV
- **Updated SECURITY.md** — Permission validation and language isolation
- **Created LANGUAGES.md** — Complete translation guide and language reference

---

## [1.4.1] - 2026-03-05

### Improvements

- **Enhanced Command UX** — `/language` shows current language, `/setlang` provides clear feedback
- **Enhanced Settings UX** — Clearer status display, better user feedback, robust callback handling
- **Automatic Alternative Frontends** — Removed manual `/redirect`, now auto-detected after URL cleaning
- **Command Cleanup** — Removed duplicate `/topusers` (merged into `/leaderboard`)

### Breaking Changes

- Removed `/topusers` command (use `/leaderboard`)
- Removed `/redirect <url>` command (now automatic)

## [1.4.0] - 2026-03-04

### New Features

- **VirusTotal Integration** — Malware detection with 70+ antivirus engines
- **Enhanced URL Detection** — Fixed `has_urls` flag for message entities

### Bug Fixes

- Fixed `has_urls` flag not being set correctly, causing URLs to be skipped
- Added comprehensive debug logging

## [1.3.0] - 2026-02-24

- Gestione errori esplicita e logging avanzato
- Modularità estesa: funzioni di sanitizzazione e validazione in moduli dedicati
- Test automatizzati aggiunti per validazione input/output
- Ottimizzazione performance con cache

## [1.2.0] - 2026-01-20

### Major Modernization

- Rust 1.92+ with MSRV 1.75
- Migration from Docker to Podman
- Build optimization with LTO, single codegen unit
- Fixed all deprecated teloxide method calls

## [1.1.0] - 2026-01-10

- Smart language detection (English/Italian)
- Supabase PostgreSQL compatibility
- Multi-database support via `sqlx::Any`
- Refactored into modular library + binary structure
- Upgraded to teloxide 0.17, axum 0.8, sqlx 0.8
