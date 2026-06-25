# Architecture Overview

## Component Structure

### Core Library (`src/lib.rs`)

Public modules exposed by the library crate:

- **`presentation::telegram`** — Telegram bot handlers, command dispatcher, UI helpers, settings navigation
- **`sanitizer`** — URL cleaning engine with four complementary strategies:
  - `rule_engine.rs` — ClearURLs rules parser, regex-based cleaning, GitHub URL handling, aggressive tracker removal (wrapped in `Mutex` for lazy loading)
  - `ai_engine.rs` — Optional OpenAI-compatible API for complex tracking patterns
  - `entropy.rs` — Shannon entropy analysis for detecting unknown tracking parameters (threshold > 3.0 bits/char, UUID/Base64/hex-hash pattern detection)
  - `multi_source.rs` — Multi-source sanitizer wrapping `url-sanitize-core` with ClearURLs + AdGuard + Brave + Firefox rules
  - `normalize.rs` — URL canonicalization via `url-normalize` (lowercase, remove www, sort params, strip UTM)
  - `validation.rs` — URL validation with in-memory caching
- **`redirects`** — Alternative frontend detection
  - `service.rs` — LibRedirect + Farside catalog lookup with host extraction
  - `models.rs` — Frontend data structures with serde deserialization
  - `cache.rs` — TTL-based catalog cache (moka)
- **`db`** — Database abstraction layer
  - `implementation.rs` — `Db` struct with all SQL operations (SQLite/PostgreSQL via `sqlx::Any`)
  - `models.rs` — Data models: `UserConfig`, `ChatConfig`, `CleanedLink`, `CustomRule`
- **`shared`** — Cross-cutting concerns:
  - `error.rs` — `AppError` enum and `AppResult<T>` type alias
  - `security.rs` — Async rate limiter (moka future cache, 1 req/sec per user), input sanitization, URL validation
- **`config`** — Environment-based configuration loading and validation
- **`metrics`** — Prometheus counters for requests, sanitizations, rate-limit hits, errors, redirect lookups, AI scans
- **`http_utils`** — HTTP retry with exponential backoff for external APIs (3 attempts, 30s max delay)
- **`i18n`** — Internationalization: 15 languages, auto-detected from Telegram client
- **`logging`** — Structured logging setup with `tracing` + `tracing-subscriber` + `EnvFilter`

### Application Entry Point (`src/main.rs`)

Minimal orchestrator (~50 lines):

1. Initialize structured logging
2. Load and validate configuration from environment variables
3. Initialize database (SQLite or PostgreSQL)
4. Create sanitizer subsystems (rule engine, AI engine, multi-source)
5. Create broadcast channel for events
6. Start the bot via `run_bot()` — long-polling (default) or webhook mode

### Module Organization

```
src/
├── lib.rs                  # Library crate — module declarations
├── main.rs                 # Application entry point (orchestrator)
├── config.rs               # Configuration management
├── constants.rs            # Application-wide constants
├── metrics.rs              # Prometheus counters
├── http_utils.rs           # HTTP retry utilities
├── i18n.rs                 # Internationalization (15 languages)
├── logging.rs              # Structured logging
├── presentation/
│   └── telegram/
│       ├── mod.rs          # Module exports (run_bot)
│       ├── handlers.rs     # Message/callback/inline dispatching
│       ├── commands.rs     # Command handler functions
│       ├── command_dispatcher.rs  # Trait-based command dispatcher
│       ├── helpers.rs      # Keyboards, UI, URL extraction
│       ├── settings.rs     # Settings callback navigation
│       └── tests.rs        # Command integration tests
├── shared/
│   ├── error.rs            # AppError, AppResult
│   ├── security.rs         # Rate limiter, sanitization, validation
│   ├── types.rs            # Shared types
│   └── url_processor.rs    # URL cleaning coordination
├── sanitizer/
│   ├── mod.rs              # Module exports
│   ├── rule_engine.rs      # ClearURLs rules parser
│   ├── ai_engine.rs        # Optional AI sanitization
│   ├── entropy.rs          # Shannon entropy tracker detection
│   ├── multi_source.rs     # Multi-source rule engine
│   ├── normalize.rs        # URL canonicalization
│   └── validation.rs       # URL validation cache
├── redirects/
│   ├── mod.rs              # Module exports
│   ├── service.rs          # LibRedirect + Farside lookup
│   ├── models.rs           # Frontend data structures
│   └── cache.rs            # TTL-based catalog cache
└── db/
    ├── mod.rs              # Module exports
    ├── implementation.rs   # Db struct with SQL operations
    └── models.rs           # Data models
```

## Data Flow

```
┌──────────────┐    ┌─────────────────┐    ┌──────────────────┐
│ Telegram API │───▶│  Bot Handlers   │───▶│  URL Detection   │
└──────────────┘    └─────────────────┘    └──────────────────┘
                           │                         │
                           ▼                         ▼
                    ┌──────────────┐        ┌──────────────────┐
                    │  Rate Limit  │        │  Sanitization    │
                    │  Check       │        │  Pipeline        │
                    └──────────────┘        └──────────────────┘
                                                     │
                    ┌──────────────┐                 ▼
                    │  Metrics     │◀───────┌──────────────────┐
                    │  Counters    │        │  Alternative     │
                    └──────────────┘        │  Frontends       │
                                            └──────────────────┘
                                                     │
                    ┌──────────────┐                 ▼
                    │  Response    │◀───────┌──────────────────┐
                    │  to User     │        │  DB Logging      │
                    └──────────────┘        └──────────────────┘
```

### Processing Pipeline

1. **Message Reception** — Long-polling (default) or webhook (`WEBHOOK_URL` + `WEBHOOK_SECRET`)
2. **Rate Limiting** — Per-user check via async moka cache (1 req/s)
3. **URL Detection** — MessageEntity (Url, TextLink) + regex fallback
4. **Context Analysis** — Language detection (Telegram client → English), user/chat config lookup
5. **Sanitization** — Multi-source rules → normalization → rule engine → entropy analysis → AI engine (optional)
6. **Alternative Frontends** — LibRedirect + Farside lookup
7. **Metrics** — Counter increments for requests, sanitization results, errors
8. **Persistence** — Audit logging, statistics, user preferences
9. **Response** — Formatted message with cleaned URLs

## URL Sanitization Pipeline

The sanitizer applies up to four complementary strategies in order:

1. **Multi-source rules** (`multi_source.rs`) — Queries `url-sanitize-core` catalog (ClearURLs + AdGuard + Brave + Firefox merged rules) for known tracking parameters, redirect unwrapping, and blocked URLs
2. **Normalization** (`normalize.rs`) — Canonicalizes URL via `url-normalize`: lowercase host, remove default ports, strip `www`, sort query parameters, remove UTM parameters, decode unnecessary percent-encoding
3. **Rule engine** (`rule_engine.rs`) — Applies ClearURLs ruleset for provider-specific parameter stripping (Amazon, Google, Facebook, TikTok, etc.)
4. **Entropy analysis** (`entropy.rs`) — Detects unknown tracking parameters via Shannon entropy (>3.0 bits/char) and pattern matching (UUID, Base64, hex hash heuristic); functional parameters (page, limit, q) are always preserved
5. **AI engine** (`ai_engine.rs`) — Optional OpenAI-compatible API pass for edge cases

Results are cached in-process (moka, 10k capacity, 1h TTL).

## Database Schema

### Supported Backends

- **SQLite** — Default for development and small deployments
- **PostgreSQL** — Recommended for production with high concurrency

Both are supported via `sqlx::Any` with automatic backend detection from the connection string.

### Tables

| Table | Purpose |
|-------|---------|
| `user_configs` | User preferences (enabled, AI toggle, mode, privacy, ignored domains, cleaned count) |
| `chat_configs` | Group chat settings (enabled, mode, added_by) |
| `cleaned_links` | URL audit log (original_url, cleaned_url, provider, timestamp) |
| `custom_rules` | User-defined regex patterns |
| `whitelist_urls` | Trusted domains per user |
| `feature_flags` | Per-user feature enablement |

Tables are created with `CREATE TABLE IF NOT EXISTS` — data persists across restarts.

## Internationalization

The bot supports 15 languages with automatic detection. The resolution pipeline:

1. **Telegram client** — uses the language code from the user's Telegram app (highest priority)
2. **Default** — English if nothing matches

Language is detected on every interaction — there is no stored preference.

Full language list and translation guide: [LANGUAGES.md](../LANGUAGES.md)

## Observability

### Metrics

A `/metrics` HTTP endpoint (Prometheus text format) exposes 11 atomic counters:

| Counter | Description |
|---------|-------------|
| `REQUESTS_MESSAGE` | Text messages received |
| `REQUESTS_INLINE` | Inline queries received |
| `REQUESTS_CALLBACK` | Callback queries received |
| `REQUESTS_EDITED` | Edited messages received |
| `SANITIZATIONS_CLEANED` | URLs successfully cleaned |
| `SANITIZATIONS_UNCHANGED` | URLs already clean (no change) |
| `SANITIZATIONS_REDIRECTED` | Redirects unwrapped |
| `SANITIZATIONS_BLOCKED` | URLs blocked by rules |
| `RATE_LIMIT_HITS` | Rate-limited requests |
| `ERRORS_TOTAL` | Internal errors |
| `REDIRECT_LOOKUPS` | Alternative frontend lookups |
| `AI_SANITIZATIONS` | AI-powered sanitizations |

### Logging

Structured logging via `tracing` + `tracing-subscriber` with `EnvFilter` for dynamic level control.

## Performance Optimizations

### Build

- **LTO** — `"fat"` for maximum cross-crate optimization
- **strip** — `"symbols"` to reduce binary size
- **codegen-units = 1** — single codegen unit for better optimization
- **panic = "abort"** — smaller panic handler
- **opt-level = "s"** — size-optimized release binary

### Runtime

- **Async I/O** — non-blocking operations throughout (tokio + teloxide)
- **Connection pooling** — database connection reuse (sqlx pool)
- **In-process caching** — moka cache for URL cleaning results (10k capacity, 1h TTL)
- **Catalog caching** — multi-source catalog and LibRedirect/Farside data cached with TTL
- **Lazy rules loading** — ClearURLs rules fetched on first use, not at startup
- **HTTP retry** — exponential backoff (3 attempts, 1s–30s) for all external API calls

## Security Architecture

### Input Validation & Sanitization

- **Rate limiting** — async moka future cache, 1 request/second per user (`shared/security.rs`)
- **Input sanitization** — control character stripping, 4000-char cap
- **Callback sanitization** — same as input, for callback query data
- **URL validation** — regex + malicious pattern detection (javascript:, data:, vbscript:, etc.)
- **Telegram text escaping** — HTML entity encoding for `<`, `>`, `&`, `"`, `'`
- **Domain validation** — RFC-compliant domain format checking

### Data Protection

- No sensitive data in logs — tokens, keys, and personal data are never logged
- Automatic redaction — sensitive URL parameters redacted in debug output
- `.env` should have restrictive permissions (`chmod 600`)
- Multi-tenant isolation — all data scoped by `user_id`

## Development

### Toolchain

- **Minimum Rust**: 1.88 (MSRV, set in `Cargo.toml`)
- **Edition**: 2024

### Code Organization Principles

- **Single Responsibility** — each module has a focused purpose
- **Async/Await** — consistent async patterns via tokio + teloxide
- **Error Handling** — `AppError` enum with `AppResult<T>` type alias, no panics, no `anyhow`
- **Logging** — `tracing` with structured fields, `EnvFilter` for dynamic level control
- **Metrics** — atomic counters, never blocking, zero allocations in hot path

### Testing

- **Unit tests** inline in each source file (`#[cfg(test)]`)
- **Integration tests** in `tests/` and inline
- **Property-based tests** via `proptest` for URL normalization invariants (idempotency, scheme preservation, UTM removal, domain preservation)
- Integration tests use isolated in-memory SQLite databases (`sqlite:file:testdb{id}?mode=memory&cache=shared`)
- Tests run in parallel
