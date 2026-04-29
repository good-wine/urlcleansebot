# Architecture Overview

This project is designed with modularity, performance, and security in mind.

## Component Structure

### 1. Core Library (`src/lib.rs`)

Public modules exposed by the library crate:

- **`presentation::telegram`** — Telegram bot handlers, UI helpers, settings navigation, security scans
- **`sanitizer`** — URL cleaning engine
  - `rule_engine.rs` — ClearURLs rules parser, regex-based cleaning, GitHub URL handling, aggressive tracker removal
  - `ai_engine.rs` — Optional OpenAI-compatible API for complex tracking patterns
  - `validation.rs` — URL validation with in-memory caching
- **`redirects`** — Alternative frontend detection
  - `service.rs` — LibRedirect + Farside catalog lookup with host extraction
  - `models.rs` — Frontend data structures with serde deserialization
  - `cache.rs` — TTL-based catalog cache (moka)
- **`db`** — Database abstraction layer
  - `implementation.rs` — `Db` struct with all SQL operations (SQLite/PostgreSQL via `sqlx::Any`)
  - `models.rs` — Data models: `UserConfig`, `ChatConfig`, `CleanedLink`, `CustomRule`
- **`application`** — Clean Architecture use cases (preserved for future refactoring)
- **`domain`** — Business entities and repository interfaces
- **`infrastructure`** — Repository implementations
- **`shared`** — Cross-cutting concerns: `AppError`, security utilities, shared types
- **`config`** — Environment-based configuration loading and validation
- **`health`** — Health check structs (healthy/unhealthy/degraded)
- **`http_utils`** — HTTP retry with exponential backoff for external APIs
- **`i18n`** — Internationalization: 15 languages (IT, EN, ES, FR, DE, PT, RU, AR, HI, ZH, JA, KO, TR, NL, PL) with auto-detection
- **`logging`** — Structured logging setup with `tracing`

### 2. Application Entry Point (`src/main.rs`)

Minimal orchestrator (~50 lines):

1. Initialize structured logging
2. Load and validate configuration from environment variables
3. Initialize database (SQLite or PostgreSQL)
4. Create `RuleEngine` (lazy, fetches rules on first use) and `AiEngine`
5. Create broadcast channel for events
6. Start the bot via `run_bot()`

### 3. Module Organization

```
src/
├── lib.rs              # Library crate — module declarations
├── main.rs             # Application entry point (orchestrator)
├── config.rs           # Configuration management
├── health.rs           # Health check types
├── http_utils.rs       # HTTP retry utilities
├── i18n.rs             # Internationalization (15 languages)
├── logging.rs          # Structured logging
├── presentation/       # User interface layer
│   └── telegram/       # Telegram-specific code
│       ├── handlers.rs       # Message/callback/inline dispatching
│       ├── helpers.rs        # Keyboards, UI, URL extraction, 13 tests
│       ├── settings.rs       # Settings callback navigation
│       ├── security_scan.rs  # VirusTotal + URLScan.io
│       └── mod.rs            # Exports run_bot()
├── application/        # Clean Architecture use cases
│   ├── commands/       # Command traits and handlers
│   ├── queries/        # Query traits and handlers
│   └── services/       # Service trait definitions
├── domain/             # Business entities and repository interfaces
│   ├── entities/       # User, ChatConfig, UrlToClean, etc.
│   └── repositories/   # Repository trait definitions
├── infrastructure/     # External implementations
│   └── repositories/   # Database repository implementations
├── shared/             # Cross-cutting concerns
│   ├── error.rs        # AppError, AppResult
│   ├── security.rs     # Rate limiter, sanitization, validation
│   └── types.rs        # Shared types
├── sanitizer/          # URL sanitization engine
├── redirects/          # Alternative frontend detection
└── db/                 # Database layer
```

## Data Flow

```
┌──────────────┐    ┌─────────────────┐    ┌──────────────────┐
│ Telegram API │───▶│  Bot Handler    │───▶│  URL Detection   │
└──────────────┘    └─────────────────┘    └──────────────────┘
                                                     │
                                                     ▼
┌──────────────┐    ┌─────────────────┐    ┌──────────────────┐
│  Response    │◀───│  DB (logging)   │◀───│  Sanitization    │
└──────────────┘    └─────────────────┘    └──────────────────┘
                                                  │
                       ┌──────────────┐           ▼
                       │  Redirects   │◀───┌──────────────────┐
                       └──────────────┘    │  Security Scan   │
                                           │  (VT/URLScan)    │
                                           └──────────────────┘
```

### Processing Pipeline

1. **Message Reception** — Long-polling (default) or webhook (`WEBHOOK_URL` + `WEBHOOK_SECRET`)
2. **URL Detection** — MessageEntity (Url, TextLink) + regex fallback
3. **Context Analysis** — Language detection, user/chat config lookup
4. **Security Check** — Optional VirusTotal + URLScan.io scan
5. **Sanitization** — Rule engine → AI engine (optional) → aggressive tracker fallback
6. **Alternative Frontends** — LibRedirect + Farside lookup
7. **Persistence** — Audit logging, statistics, user preferences
8. **Response** — Formatted message with cleaned URLs and optional security warnings

## Database Schema

### Supported Backends

- **SQLite** — Default for development and small deployments
- **PostgreSQL** — Recommended for production with high concurrency

Both are supported via `sqlx::Any` with automatic backend detection from the connection string.

### Tables

| Table | Purpose |
|-------|---------|
| `user_configs` | User preferences (enabled, AI toggle, mode, language, privacy, ignored domains, cleaned count) |
| `chat_configs` | Group chat settings (enabled, mode, added_by) |
| `cleaned_links` | URL audit log (original_url, cleaned_url, provider, timestamp) |
| `custom_rules` | User-defined regex patterns |
| `whitelist_urls` | Trusted domains per user (with UNIQUE constraint) |
| `feature_flags` | Per-user feature enablement (PRIMARY KEY user_id + feature_name) |
| `rate_limits` | Sliding-window rate limiting (action_count, window_start) |

Tables are created with `CREATE TABLE IF NOT EXISTS` — data persists across restarts.

## Internationalization

### Supported Languages

| Code | Language | Native Name | Code | Language | Native Name |
|------|----------|-------------|------|----------|-------------|
| `it` | Italian | Italiano | `ar` | Arabic | العربية |
| `en` | English | English | `hi` | Hindi | हिन्दी |
| `es` | Spanish | Español | `zh` | Chinese | 中文 |
| `fr` | French | Français | `ja` | Japanese | 日本語 |
| `de` | German | Deutsch | `ko` | Korean | 한국어 |
| `pt` | Portuguese | Português | `tr` | Turkish | Türkçe |
| `ru` | Russian | Русский | `nl` | Dutch | Nederlands |
| `pl` | Polish | Polski | | | |

### Detection Pipeline

1. **User preference** — saved language from `/setlang` or settings menu (highest priority)
2. **Content detection** — `whatlang` crate analyzes message text (supports all 15 languages)
3. **Telegram client** — falls back to the user's Telegram language code
4. **Default** — English if nothing matches

The language selector is rendered as a 4-column inline keyboard grid, with all 15 languages available at once plus a back button.

See [LANGUAGES.md](../LANGUAGES.md) for the translation guide.

## Performance Optimizations

### Build

- **LTO** — `"fat"` for maximum cross-crate optimization
- **strip** — `"symbols"` to reduce binary size
- **codegen-units = 1** — single codegen unit for better optimization
- **panic = "abort"** — smaller panic handler
- **opt-level = "z"** — size-optimized release binary

### Runtime

- **Async I/O** — non-blocking operations throughout
- **Connection pooling** — database connection reuse
- **In-process caching** — moka cache for URL cleaning results (10k capacity, 1h TTL)
- **Catalog caching** — LibRedirect/Farside data cached with 6h TTL
- **Lazy rules loading** — ClearURLs rules fetched on first use, not at startup
- **HTTP retry** — exponential backoff for all external API calls

## Security Architecture

### Input Validation & Sanitization

- **Rate limiting** — moka sync cache, 1 request/second per user
- **Input sanitization** — control character stripping, 4000-char cap
- **Callback sanitization** — same as input, for callback query data
- **URL validation** — regex + malicious pattern detection (javascript:, data:, etc.)
- **Telegram text escaping** — HTML entity encoding for `<`, `>`, `&`, `"`, `'`

### External Security Integrations

- **VirusTotal API** — Real-time malware detection with 70+ antivirus engines
- **URLScan.io** — Behavioral analysis and web reputation scoring
- Both are optional and disabled by default

### Container Security

- Rootless Podman execution
- Non-root user in container
- SELinux file labeling

## Development

### Toolchain

- **Minimum Rust**: 1.88 (MSRV, set in `Cargo.toml`)
- **Edition**: Rust 2021

### Code Organization Principles

- **Single Responsibility** — each module has a focused purpose
- **Async/Await** — consistent async patterns
- **Error Handling** — `AppError` enum with `AppResult<T>` type alias
- **No panics in production code** — graceful error propagation

### Testing

- **Unit tests** — inline `#[cfg(test)]` modules in each source file
- **Integration tests** — `tests/` directory with in-memory SQLite
- **110 total tests** — 63 unit + 8 bot commands + 10 database + 9 sanitizer + 20 security
- Tests run in parallel with isolated databases
