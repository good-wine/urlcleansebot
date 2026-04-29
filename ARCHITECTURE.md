# ClearURLs Bot — Architecture

## Overview

Telegram bot that removes tracking parameters from URLs using ClearURLs rules, with optional AI-powered sanitization and security scanning via VirusTotal/URLScan.io.

## Architecture Layers

```
src/
├── presentation/            # User interface layer
│   └── telegram/            # Telegram Bot API handlers
│       ├── handlers.rs      # Message, callback, inline query dispatching (~1300 lines)
│       ├── helpers.rs       # Keyboards, UI builders, URL utils, language grid, 13 unit tests
│       ├── settings.rs      # User/admin settings callback navigation
│       ├── security_scan.rs # VirusTotal + URLScan.io integration
│       └── mod.rs           # Module exports (run_bot)
├── application/             # Use cases (Clean Architecture — preserved for future use)
│   ├── commands/            # Command handlers and traits
│   ├── queries/             # Query handlers and traits
│   └── services/            # Service trait definitions
├── domain/                  # Business logic entities
│   ├── entities/            # User, ChatConfig, UrlToClean, etc.
│   └── repositories/        # Repository interfaces
├── infrastructure/          # External implementations
│   └── repositories/        # Database repository impls (PostgresUserRepository, etc.)
├── shared/                  # Cross-cutting concerns
│   ├── error.rs             # AppError enum and AppResult<T>
│   ├── security.rs          # Rate limiter, input sanitization, URL validation
│   └── types.rs             # Common shared types
├── sanitizer/               # URL sanitization engine
│   ├── rule_engine.rs       # ClearURLs rules parser + regex cleaning
│   ├── ai_engine.rs         # Optional AI-powered sanitization
│   └── validation.rs        # URL validation with caching
├── redirects/               # Alternative frontend detection
│   ├── service.rs           # LibRedirect + Farside lookup
│   ├── models.rs            # Frontend data structures
│   └── cache.rs             # TTL-based catalog cache
├── db/                      # Database layer
│   ├── implementation.rs    # Db struct with all SQL queries (SQLite/PostgreSQL)
│   └── models.rs            # UserConfig, ChatConfig, CleanedLink, CustomRule
├── config.rs                # Environment-based configuration
├── health.rs                # HealthCheck struct (healthy/unhealthy/degraded)
├── http_utils.rs            # HTTP retry with exponential backoff
├── i18n.rs                  # Internationalization (15 languages)
├── logging.rs               # Structured logging with tracing
└── main.rs                  # Entry point — pure orchestrator (~50 lines)
```

## Data Flow

```
main.rs → run_bot() → Dispatcher → Telegram handlers
                                          │
                                          ├── DB (user/chat config)
                                          ├── RuleEngine (ClearURLs rules)
                                          ├── AiEngine (optional)
                                          ├── RedirectService (alternative frontends)
                                          └── security_scan (VirusTotal/URLScan)
```

## URL Sanitization Pipeline

1. **Extract** URLs from Telegram message entities + regex fallback
2. **Expand** shortened URLs (bit.ly, t.co, etc.)
3. **Check** against VirusTotal + URLScan.io (security)
4. **Sanitize** using ClearURLs rules
5. **Optional AI** pass for provider-specific parameters
6. **Cache** results in-process (moka, 10k capacity)

## Security

- **Rate limiting**: moka sync cache with TTL per user ID (1 req/sec)
- **Input sanitization**: callback data validation, control character stripping, length caps
- **URL validation**: regex + malicious pattern detection in `shared/security.rs`
- **Whitelist**: user-specific trusted domains (skips security scans)
- **Multi-tenant**: all data scoped by user_id

## Database

Supports SQLite (default) and PostgreSQL via `sqlx::Any`:

| Table | Purpose |
|-------|---------|
| `user_configs` | User preferences (language, mode, AI toggle, privacy) |
| `chat_configs` | Group chat settings |
| `cleaned_links` | URL history with timestamps |
| `whitelist_urls` | Trusted domains per user |
| `custom_rules` | User-defined cleaning rules |
| `feature_flags` | Per-user feature enablement |
| `rate_limits` | Sliding-window rate limiting |

Tables are created with `CREATE TABLE IF NOT EXISTS` — data persists across restarts.

## Deployment

### Modes

- **Long-polling** (default) — no webhook required
- **Webhook** — set `WEBHOOK_URL` + `WEBHOOK_SECRET`

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TELOXIDE_TOKEN` | Yes | — | Telegram bot token |
| `BOT_USERNAME` | Yes | — | Bot username |
| `ADMIN_ID` | No | `0` | Admin user ID |
| `DATABASE_URL` | No | `sqlite:bot.db` | Database connection |
| `PORT` | No | `8080` | Webhook server port |
| `VIRUSTOTAL_API_KEY` | No | — | VirusTotal API key |
| `URLSCAN_API_KEY` | No | — | URLScan.io API key |
| `AI_API_KEY` | No | — | OpenAI-compatible API key |
| `WEBHOOK_URL` | No | — | Public webhook URL |
| `WEBHOOK_SECRET` | Conditional | — | Required if WEBHOOK_URL set |

## Internationalization

The bot supports 15 languages with auto-detection from message content (via `whatlang`) and Telegram client settings:

| Code | Language | Code | Language | Code | Language |
|------|----------|------|----------|------|----------|
| `it` | Italian | `es` | Spanish | `de` | German |
| `fr` | French | `pt` | Portuguese | `ru` | Russian |
| `ar` | Arabic | `hi` | Hindi | `zh` | Chinese |
| `ja` | Japanese | `ko` | Korean | `tr` | Turkish |
| `nl` | Dutch | `pl` | Polish | `en` | English (default) |

Language detection pipeline:
1. **User preference** — saved language from settings (highest priority)
2. **Content detection** — `whatlang` crate analyzes message text
3. **Telegram client** — falls back to the user's Telegram language code
4. **Default** — English if nothing matches

See [LANGUAGES.md](LANGUAGES.md) for the full translation guide.

## Development

```bash
cargo check                                    # compile check
cargo clippy --all-targets -- -D warnings      # lint
cargo test                                     # 90 tests (63 unit + 27 integration)
cargo fmt --all                                # format
```

CI runs on every push/PR: `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib`, `cargo test --doc`.
