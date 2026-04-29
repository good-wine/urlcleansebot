# ClearURLs Telegram Bot

[![CI](https://github.com/good-wine/clearurlsbot/actions/workflows/ci.yml/badge.svg)](https://github.com/good-wine/clearurlsbot/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.88+-orange.svg)](https://www.rust-lang.org)
[![Podman](https://img.shields.io/badge/Podman-supported-blue.svg)](https://podman.io)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A modern, high-performance Rust-based Telegram bot that automatically removes tracking parameters from URLs.

## Key Features

- **ClearURLs Rule Engine** — Downloads and applies the official ClearURLs ruleset to strip tracking parameters
- **AI Deep Scan** (optional) — OpenAI-compatible API pass for complex tracking patterns
- **Shortlink Expansion** — Follows redirects from bit.ly, tinyurl, etc. to uncover hidden trackers
- **Alternative Frontends** — Auto-detects URLs from YouTube, Twitter, Reddit, etc. and suggests privacy-focused alternatives (Invidious, Piped, Libretto, Nitter, Teddit) via LibRedirect and Farside
- **Security Scanning** — VirusTotal and URLScan.io integration for malware/reputation checks
- **Multi-language** — 15 languages with auto-detection (IT, EN, ES, FR, DE, PT, RU, AR, HI, ZH, JA, KO, TR, NL, PL)
- **Inline Mode** — Clean URLs directly from the inline query bar
- **Group Support** — Per-chat configuration (Reply/Delete modes)
- **Statistics & Leaderboards** — Personal stats, domain breakdowns, top users, trending links
- **Feature Flags & Rate Limiting** — Database-level per-user feature control and abuse protection

## Quick Start

### 1. Clone & Configure

```bash
git clone https://github.com/good-wine/clearurlsbot.git
cd clearurlsbot
cp .env.example .env
```

Edit `.env` with your settings:

```ini
TELOXIDE_TOKEN=your_bot_token
BOT_USERNAME=@your_bot_username
ADMIN_ID=your_telegram_user_id

# Optional
AI_API_KEY=your_openai_key
VIRUSTOTAL_API_KEY=your_vt_key
URLSCAN_API_KEY=your_urlscan_key
WEBHOOK_URL=https://your-domain.com/webhook
WEBHOOK_SECRET=your_secret
```

### 2. Run

```bash
cargo run              # development
cargo run --release    # production
```

### 3. Deploy with Podman

```bash
./podman-deploy.sh start
# or
podman-compose -f podman-compose.yml up
```

## Bot Commands

| Command | Description |
|---------|-------------|
| `/start` | Initialize the bot |
| `/help` | Show help |
| `/menu` | Quick reply keyboard |
| `/settings` | Interactive settings menu |
| `/stats` | Personal statistics |
| `/history` | Last 10 cleaned URLs |
| `/domains` | Stats grouped by domain |
| `/leaderboard` | Top 10 users |
| `/trending` | Most frequently cleaned URLs |
| `/export` | Export data as JSON |
| `/whitelist` | Manage whitelisted domains |
| `/limits` | Check rate limits |
| `/language` | Show available languages |
| `/setlang <code>` | Change language (it, en, es, fr, de, pt, ru, ar, hi, zh, ja, ko, tr, nl, pl) |
| `/hidekbd` | Hide reply keyboard |

## Architecture

The codebase follows a layered structure:

```
src/
├── presentation/telegram/   # Telegram handlers, UI helpers, settings, security scans
├── sanitizer/               # URL cleaning engine (RuleEngine, AiEngine)
├── redirects/               # Alternative frontend detection (LibRedirect, Farside)
├── db/                      # Database layer (SQLite/PostgreSQL via sqlx::Any)
├── application/             # Clean Architecture skeleton (commands, queries, services)
├── domain/                  # Business entities and repository interfaces
├── infrastructure/          # Repository implementations
├── shared/                  # Cross-cutting: error types, security utils, types
├── config.rs                # Environment-based configuration
├── health.rs                # Health check structs
├── http_utils.rs            # HTTP retry with exponential backoff
├── i18n.rs                  # Internationalization (15 languages)
├── logging.rs               # Structured logging setup
└── main.rs                  # Orchestrator (~50 lines)
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full overview and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the deep dive.

## Supported Languages

| Code | Language | Native Name |
|------|----------|-------------|
| `it` | Italian | Italiano |
| `en` | English | English |
| `es` | Spanish | Español |
| `fr` | French | Français |
| `de` | German | Deutsch |
| `pt` | Portuguese | Português |
| `ru` | Russian | Русский |
| `ar` | Arabic | العربية |
| `hi` | Hindi | हिन्दी |
| `zh` | Chinese | 中文 |
| `ja` | Japanese | 日本語 |
| `ko` | Korean | 한국어 |
| `tr` | Turkish | Türkçe |
| `nl` | Dutch | Nederlands |
| `pl` | Polish | Polski |

Languages are auto-detected from message content or Telegram client settings, and can be manually set via `/setlang <code>` or the settings menu.

See [LANGUAGES.md](LANGUAGES.md) for the full translation guide.

## Documentation

| Document | Description |
|----------|-------------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | High-level architecture overview |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Detailed architecture deep dive |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) | Deployment guides |
| [docs/VIRUSTOTAL.md](docs/VIRUSTOTAL.md) | VirusTotal integration |
| [docs/URLSCAN.md](docs/URLSCAN.md) | URLScan.io integration |
| [docs/SCAN_CACHING.md](docs/SCAN_CACHING.md) | Security scan caching |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |
| [QUICK_START.md](QUICK_START.md) | Step-by-step setup guide (Italian) |
| [LANGUAGES.md](LANGUAGES.md) | Supported languages & translation guide |
| [SECURITY.md](SECURITY.md) | Security policy |
| [CHANGELOG.md](CHANGELOG.md) | Release history |
| [COMPILATION_GUIDE.md](COMPILATION_GUIDE.md) | Build optimization guide |

## Development

```bash
cargo check              # compile check
cargo clippy --all-targets -- -D warnings   # lint
cargo test               # run all tests (72 total)
cargo fmt --all          # format
```

### Test Structure

- **45 unit tests** — sanitizer, redirects, security, helpers, health
- **8 bot command tests** — integration tests with in-memory SQLite
- **10 database tests** — user configs, history, whitelist, feature flags
- **9 sanitizer tests** — real ClearURLs rules fetching and URL cleaning

All tests use isolated in-memory SQLite databases and run in parallel.

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `TELOXIDE_TOKEN` | Yes | — | Telegram bot token |
| `BOT_USERNAME` | Yes | — | Bot username (with or without @) |
| `ADMIN_ID` | No | `0` | Admin Telegram user ID |
| `DATABASE_URL` | No | `sqlite:bot.db` | SQLite or PostgreSQL connection |
| `PORT` | No | `8080` | Webhook server port |
| `SERVER_ADDR` | No | `0.0.0.0:{PORT}` | Bind address |
| `CLEARURLS_SOURCE` | No | [official rules](https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json) | ClearURLs rules URL |
| `LIBREDIRECT_URL` | No | [libredirect instances](https://raw.githubusercontent.com/libredirect/instances/main/data.json) | LibRedirect catalog |
| `FARSIDE_URL` | No | [farside services](https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json) | Farside catalog |
| `AI_API_KEY` | No | — | OpenAI-compatible API key |
| `AI_API_BASE` | No | `https://api.openai.com/v1` | AI API base URL |
| `AI_MODEL` | No | `gpt-3.5-turbo` | AI model name |
| `INLINE_MAX_RESULTS` | No | `5` | Max inline results (1-50) |
| `VIRUSTOTAL_API_KEY` | No | — | VirusTotal API key |
| `URLSCAN_API_KEY` | No | — | URLScan.io API key |
| `WEBHOOK_URL` | No | — | Public HTTPS webhook URL |
| `WEBHOOK_SECRET` | Conditional | — | Required if WEBHOOK_URL set |

## License

MIT — see [LICENSE](LICENSE).
