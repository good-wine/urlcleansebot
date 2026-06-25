# Architecture Overview

## Component Structure

### Core Library (`src/lib.rs`)

Public modules exposed by the library crate:

- **`presentation::telegram`** — Telegram bot handlers, command dispatcher, UI helpers, settings navigation
  - `handlers/` — Message/callback/inline dispatch split into focused modules
- **`sanitizer`** — URL cleaning engine with multiple complementary strategies:
  - `rule_engine/` — ClearURLs rules subsystem (submodules: `clearurls` parser, `expand` short URL, `ssrf` protection, `redact` sensitive data, `github` URL truncation)
  - `ai_engine.rs` — Optional OpenAI-compatible API for complex tracking patterns
  - `entropy.rs` — Shannon entropy analysis for detecting unknown tracking parameters (threshold > 3.0 bits/char, UUID/Base64/hex-hash pattern detection)
  - `multi_source.rs` — Multi-source sanitizer wrapping `url-sanitize-core` with ClearURLs + AdGuard + Brave + Firefox rules
  - `normalize.rs` — URL canonicalization via `url-normalize` (lowercase, remove www, sort params, strip UTM)
  - `pipeline.rs` — Orchestrator chaining all sanitization steps
  - `aggressive.rs` — Aggressive tracking parameter removal fallback
  - `honor_creator.rs` — Affiliate link preservation
  - `classifier.rs` — Tracking vs functional parameter classifier
  - `linkumori.rs` — Linkumori community rules
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
  - `security.rs` — Async rate limiter (moka future cache, 1 req/sec per user), input sanitization, URL validation, HMAC
  - `ports/` — Trait interfaces for dependency inversion and testability:
    - `DatabasePort` — `#[async_trait]` with mockall automock, all DB operations (user config, rules, whitelist, history, feature flags, ping)
    - `SanitizerService` — URL sanitization + expansion + ClearURLs support check
    - `AiProvider` — AI sanitization with `AiError` enum
    - `RedirectProvider` — Alternative frontend lookup + domain support check
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
├── lib.rs                  # Library crate — module declarations + deny(unused_crate_dependencies)
├── main.rs                 # Application entry point (orchestrator)
├── config.rs               # Environment-based configuration (serde)
├── constants.rs            # Application-wide constants
├── metrics.rs              # Prometheus counters (Counter, Histogram, Gauge)
├── http_utils.rs           # HTTP retry with exponential backoff
├── i18n/                   # Internationalization (15 languages)
│   ├── mod.rs              # Translations struct + get_translations()
│   └── it.rs, en.rs, ...   # Per-language modules
├── logging.rs              # Structured tracing + optional OTLP exporter
├── presentation/
│   └── telegram/
│       ├── mod.rs          # Module exports (run_bot)
│       ├── handlers/       # Bot message/callback/inline dispatch
│       │   ├── mod.rs      # run_bot(), health/metrics HTTP handlers
│       │   ├── message.rs  # handle_message, handle_edited_message
│       │   ├── inline.rs   # handle_inline_query, handle_chosen_inline_result
│       │   └── callback.rs # handle_callback + dedup cache
│       ├── commands.rs     # Command handler functions
│       ├── command_dispatcher.rs  # Trait-based command dispatcher
│       ├── helpers.rs      # Keyboards, UI, URL extraction
│       ├── settings.rs     # Settings callback navigation
│       └── tests.rs        # Command integration tests (stubs)
├── shared/
│   ├── error.rs            # AppError, AppResult
│   ├── security.rs         # Rate limiter, sanitization, validation, HMAC
│   └── ports/              # Trait interfaces for testability
│       ├── mod.rs          # Public exports + mockall mock re-exports
│       ├── database.rs     # DatabasePort trait (+ MockDatabasePort)
│       ├── sanitizer.rs    # SanitizerService trait (+ MockSanitizerService)
│       ├── ai.rs           # AiProvider trait (+ MockAiProvider)
│       └── redirect.rs     # RedirectProvider trait (+ MockRedirectProvider)
├── sanitizer/
│   ├── mod.rs              # Module exports
│   ├── rule_engine/        # ClearURLs rules subsystem
│   │   ├── mod.rs          # RuleEngine struct, sanitize(), clean_url_in_place()
│   │   ├── clearurls.rs    # RawProvider, CompiledProvider, compile_providers()
│   │   ├── expand.rs       # Short URL expansion with SSRF guard
│   │   ├── github.rs       # GitHub URL truncation
│   │   ├── redact.rs       # Sensitive data redaction
│   │   └── ssrf.rs         # Private/reserved IP detection
│   ├── ai_engine.rs        # Optional OpenAI-compatible API
│   ├── entropy.rs          # Shannon entropy tracker detection
│   ├── multi_source.rs     # Multi-source rules wrapper
│   ├── normalize.rs        # URL canonicalization (url-normalize)
│   ├── pipeline.rs         # Sanitization pipeline orchestrator + response builder
│   ├── aggressive.rs       # Aggressive tracking parameter removal
│   ├── honor_creator.rs    # Affiliate link preservation
│   ├── classifier.rs       # Tracking vs functional parameter classifier
│   ├── linkumori.rs        # Linkumori community rules
│   └── validation.rs       # URL validation cache
├── redirects/
│   ├── mod.rs              # Module exports
│   ├── service.rs          # LibRedirect + Farside lookup
│   ├── models.rs           # Frontend data structures (Frontend, LookupHit)
│   └── cache.rs            # TTL-based catalog cache
└── db/
    ├── mod.rs              # Module exports
    ├── implementation.rs   # Db struct + SQL operations + DatabasePort impl
    ├── models.rs           # UserConfig, ChatConfig, CleanedLink, CustomRule
    └── migrations/         # SQL migration files
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

A `/metrics` HTTP endpoint (Prometheus text format) exposes counters via the `prometheus` crate:

| Metric | Type | Description |
|--------|------|-------------|
| `REQUESTS_MESSAGE` | Counter | Text messages received |
| `REQUESTS_INLINE` | Counter | Inline queries received |
| `REQUESTS_CALLBACK` | Counter | Callback queries received |
| `REQUESTS_EDITED` | Counter | Edited messages received |
| `SANITIZATIONS_CLEANED` | Counter | URLs successfully cleaned |
| `SANITIZATIONS_UNCHANGED` | Counter | URLs already clean (no change) |
| `SANITIZATIONS_REDIRECTED` | Counter | Redirects unwrapped |
| `SANITIZATIONS_BLOCKED` | Counter | URLs blocked by rules |
| `RATE_LIMIT_HITS` | Counter | Rate-limited requests |
| `ERRORS_TOTAL` | Counter | Internal errors |
| `REDIRECT_LOOKUPS` | Counter | Alternative frontend lookups |
| `AI_SANITIZATIONS` | Counter | AI-powered sanitizations |

Previously implemented with `AtomicU64`, now using `prometheus::Counter`, `Histogram`, and `Gauge` with proper encoder-based rendering via `prometheus::TextEncoder`.

### Logging

Structured logging via `tracing` + `tracing-subscriber` with `EnvFilter` for dynamic level control.

**OpenTelemetry** (optional): When `OTLP_ENDPOINT` is set, a global tracer provider is configured with OTLP exporter for distributed tracing integration (e.g., Jaeger, Grafana Tempo).

### Health Checks

The bot exposes two health endpoints on the same HTTP server:

| Endpoint | Purpose | Checks |
|----------|---------|--------|
| `GET /health` | Liveness | Always returns 200 |
| `GET /ready` | Readiness | Returns 200 if DB ping succeeds, 503 otherwise |

### Graceful Shutdown

The bot listens for `SIGTERM`/`SIGINT` via a `CancellationToken`. On signal:
1. Stops accepting new webhook requests / long-polling dispatches
2. Drains in-flight sanitization operations
3. Shuts down the tracing provider
4. Exits cleanly

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
- **Integration tests** in `tests/` directory:
  - `sanitizer_tests.rs` — Real ClearURLs rules + proptest invariants
  - `database_tests.rs` — SQLite-backed DB operations
  - `bot_commands_tests.rs` — Command handler formatting
- **Mock tests** via `mockall` — enable with `--features test-utils`:
  - `tests/trait_tests.rs` — MockDatabasePort, MockSanitizerService, MockAiProvider, MockRedirectProvider
- **Wiremock tests** — Simulated HTTP endpoints for rule download error handling:
  - `tests/wiremock_tests.rs` — 404, invalid JSON, custom rules, Amazon/TestProvider rules
- **Property-based tests** via `proptest` for URL normalization invariants (idempotency, scheme preservation, UTM removal, domain preservation)
- **Benchmarks** via `criterion` in `benches/`:
  - `benches/sanitization.rs` — URL parsing, query param removal, regex matching
  - `benches/entropy.rs` — Shannon entropy, URL encode/decode
- Integration tests use isolated in-memory SQLite databases (`sqlite:file:testdb{id}?mode=memory&cache=shared`)
- Tests run in parallel

#### Running Tests

```bash
# All tests
cargo test

# With mockall mocks (tests/trait_tests.rs)
cargo test --features test-utils --test trait_tests

# Wiremock integration tests
cargo test --test wiremock_tests

# Benchmarks
cargo bench

# Property-based (single test with many iterations)
cargo test normalize_is_idempotent
```
