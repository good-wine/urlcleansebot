# Changelog

All notable changes to URLCleanseBot will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed

- **Manual language selection** — removed `/language` and `/setlang` commands, language inline keyboard, language button from settings menu and reply keyboard
- **`language` field from `UserConfig`** — no longer persisted in database

### Changed

- **Renamed to URLCleanseBot** — package/binary renamed from `clear_urls_bot` to `url_cleanse_bot`, display name updated throughout
- **Language is now auto-detected only** — uses Telegram client `language_code` on every interaction, never a saved preference
- **`get_user_language()`** — ignores DB, only checks Telegram's language code
- **`i18n.rs`** — removed 22 language-selection fields (`s_language`, `s_language_title`, `s_language_current`, `s_language_updated`, `s_language_*` flags, `rk_language`, `cmd_language_prompt`) from all 15 locales
- **`cmd_settings_hint`** — updated in all 15 languages (no longer references `/setlang`)
- **`help_text`** — updated in all 15 languages (removed `/language` and `/setlang` references)
- **`CommandContext`** — added `lang_code` field for passing detected language to handlers

### Added

- **Lint configuration** — `[lints.rust]` with `unsafe_code = "deny"`, `[lints.clippy]` with `all = "deny"` in `Cargo.toml`
- **`rustfmt.toml`** — Explicit formatting rules (edition 2024, Unix newlines, shorthand fields, trailing commas)
- **`clippy.toml`** — MSRV 1.88, test allowances, project ident allowlist
- **`Justfile`** — Modern command runner with recipes: `check`, `clippy`, `format`, `fix`, `build`, `test`, `audit`, `deny`, `pre-commit`, `setup`
- **Pre-commit hook** — `.githooks/pre-commit` runs `check + fmt + clippy` before each commit
- **`.github/workflows/ci.yml`** — 7 parallel jobs (fmt, check, clippy, test, audit, deny, features, msrv), `concurrency` with cancel-in-progress, `taiki-e/install-action` for fast tool install, `cargo-hack` for feature-powerset check
- **Package metadata** — `description`, `license`, `repository`, `keywords`, `categories` in `Cargo.toml`
- **`.github/FUNDING.yml`** — GitHub Sponsors + PayPal funding links
- **`SUPPORT.md`** — Getting help guide

### Changed

- **`Cargo.lock` now committed** — removed from `.gitignore` for reproducible builds
- **All 58 dependencies updated** via `cargo update`
- **`CODE_OF_CONDUCT.md`** — Upgraded to Contributor Covenant v2.1 with enforcement guidelines
- **`README.md`** — Tech stack badges, Mermaid architecture diagram, try-it link, module map, related projects, star history
- **`CHANGELOG.md`** — Keep a Changelog format with semver links
- **`CONTRIBUTING.md`** — Added `just` workflow, git hooks setup, conventional commits table
- **`SECURITY.md`** — GPG-encrypted disclosure, coordinated disclosure timeline
- **`Justfile`** — Added `help` recipe

### Fixed

- `Retry::spawn` → `Retry::start` deprecation in `src/http_utils.rs`

## [1.4.1] - 2026-03-05

### Added

- **15 languages** — Arabic, Hindi, Chinese, Japanese, Korean, Turkish, Dutch, Polish
- **Refactored language selector** — 4-column grid inline keyboard
- **`/setlang` accepts all codes** — With helpful error messages
- **Auto-detection expanded** — `whatlang` maps all 15 languages
- **`LANGUAGES.md`** — Translation guide and language reference

### Fixed

- **Memory leak** — Removed `Box::leak()` in `handle_message()`
- **Port validation** — Substring match (`"8080"` in `"18080"`) → proper parsing
- **URL regex** — `.[^\s]*` (literal dot) → `[^\s]*` (any char)
- **PostgreSQL compatibility** — `get_domain_cleanup_stats()` branches on backend
- **Logging inconsistency** — `log::error!` → `tracing::warn!` in `config.rs`
- **Database persistence** — `DROP TABLE` → `CREATE TABLE IF NOT EXISTS`
- **Increment upsert** — `UPDATE` → `INSERT ... ON CONFLICT DO UPDATE`
- **Schema mismatch** — Added missing `privacy_mode` column

### Changed

- **Consolidated sanitization** — `sanitize_input()` + `sanitize_callback()` → `sanitize_string()`
- **Security modules merged** — `src/security.rs` + `src/shared/security.rs` → single `shared/security.rs`
- **Config port deduplication** — Single `PORT` default `8080`
- **Test isolation** — Unique `sqlite:file:testdb{id}` URIs per test
- **Sanitizer tests** — `RuleEngine::new_lazy()` → `RuleEngine::new().await`

## [1.4.0] - 2026-03-04

### Added

- VirusTotal integration (70+ antivirus engines)
- Enhanced URL detection via message entities

### Fixed

- `has_urls` flag not set correctly

## [1.3.0] - 2026-02-24

- Explicit error handling and advanced logging
- Sanitization and validation in dedicated modules
- Automated tests for input/output validation
- Performance optimization with cache

## [1.2.0] - 2026-01-20

### Changed

- Rust 1.92+ with MSRV 1.75
- Docker → Podman
- LTO, single codegen unit
- Fixed deprecated teloxide methods

## [1.1.0] - 2026-01-10

- Smart language detection (English/Italian)
- Supabase PostgreSQL support
- Multi-database via `sqlx::Any`
- Modular library + binary structure
- teloxide 0.17, axum 0.8, sqlx 0.8

---

[Unreleased]: https://github.com/good-wine/urlcleansebot/compare/v1.4.1...HEAD
[1.4.1]: https://github.com/good-wine/urlcleansebot/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/good-wine/urlcleansebot/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/good-wine/urlcleansebot/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/good-wine/urlcleansebot/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/good-wine/urlcleansebot/releases/tag/v1.1.0
