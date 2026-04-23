# ClearURLs Telegram Bot 🛡️

[![Rust](https://img.shields.io/badge/rust-1.92+-orange.svg)](https://www.rust-lang.org)
[![Podman](https://img.shields.io/badge/Podman-supported-blue.svg)](https://podman.io)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

A modern, high-performance Rust-based Telegram bot that automatically removes tracking parameters from URLs. Built with the latest Rust toolchain and optimized for Podman containerization.

## ✨ What's New

**🚀 Major Modernization (v0.1.0+)**

- ✅ Updated to Rust 1.92+ with optimized build configuration
- ✅ Migrated from Docker to Podman for enhanced security
- ✅ Fixed all deprecation warnings and modernized codebase
- ✅ Optimized build times and runtime performance
- ✅ Enhanced container security with rootless operation
- ✅ **NEW**: VirusTotal integration for malware detection
- ✅ **NEW**: URLScan.io integration for web reputation analysis

## 📖 Documentation

- **[Architecture Guide](docs/ARCHITECTURE.md)**: Deep dive into the modular structure and data flow
- **[Deployment Guide](docs/DEPLOYMENT.md)**: Complete deployment instructions for all environments
- **[VirusTotal Integration](docs/VIRUSTOTAL.md)**: 🆕 Malware detection setup and configuration
- **[URLScan.io Integration](docs/URLSCAN.md)**: 🆕 Web reputation and behavioral analysis
- **[Contributing](CONTRIBUTING.md)**: How to set up development and submit changes
- **[Changelog](CHANGELOG.md)**: History of releases and updates

## 🌟 Key Features

- **Smart Language Detection**: Automatically detects and responds in English or Italian based on message context and user settings
- **Multi-Language Support**: Full i18n support for Italian and English
- **Dual Security Scanning**:
  - **VirusTotal**: Real-time malware detection with 70+ antivirus engines
  - **URLScan.io**: Behavioral analysis and web reputation scoring
- **Granular Control**: Per-chat configuration (Reply/Delete modes) and custom tracking parameter removal
- **AI Deep Scan**: Optional AI-powered sanitization for complex tracking parameters not covered by standard rules
- **Shortlink Expansion**: Automatically follows redirects from services like bit.ly or tinyurl to uncover and strip underlying trackers
- **Deep Auditing**: Track which provider (Amazon, Google, etc.) cleaned each link
- **Feature Flags System**: 🆕 Gradual rollout and per-user feature control
- **Rate Limiting**: 🆕 Database-level protection against abuse
- **Health Monitoring**: 🆕 Built-in health check endpoint for production monitoring
- **Comprehensive Testing**: 🆕 Full test suite with 30+ test cases
- **CI/CD Pipeline**: 🆕 Automated testing and deployment via GitHub Actions

## 🚀 Quick Start

### Prerequisites

- **Rust 1.92+** (minimum 1.75 supported)
- **Podman** (recommended for deployment)
- **PostgreSQL or SQLite** for database

### 1. Clone & Configure

```bash
git clone https://github.com/yourusername/clear_urls_bot.git
cd clear_urls_bot
cp .env.example .env
```

Edit `.env` with your settings:

```bash
TELOXIDE_TOKEN=your_bot_token
BOT_USERNAME=@your_bot_username
ADMIN_ID=your_telegram_user_id
COOKIE_KEY=random_32_character_string

# Optional for AI Deep Scan
AI_API_KEY=your_ai_api_key
AI_API_BASE=https://api.openai.com/v1
AI_MODEL=gpt-4

# Optional: VirusTotal integration for malware detection
# Get your free API key at: https://www.virustotal.com/gui/my-apikey
# Free tier: 4 requests/minute, 500/day, 15,500/month
VIRUSTOTAL_API_KEY=your_virustotal_api_key
# Send messages only for suspicious/malicious URLs (default: true)
VIRUSTOTAL_ALERT_ONLY=true

# Optional: URLScan.io integration for web reputation analysis
# Get API key: https://urlscan.io/user/signup
# Behavioral analysis with private scans
URLSCAN_API_KEY=your_urlscan_api_key
# Send messages only for suspicious/malicious URLs (default: true)
URLSCAN_ALERT_ONLY=true

# Optional: max inline results returned by Telegram inline mode (default: 5)
INLINE_MAX_RESULTS=5
```

### 2. Run Locally

```bash
# Development build
cargo run

# Optimized release build
cargo run --release
```

### 3. Deploy with Podman (Recommended)

```bash
# Using the deployment script (recommended)
./podman-deploy.sh start

# Or with podman-compose
podman-compose -f podman-compose.yml up

# Or manually
podman build -t clear_urls_bot -f Containerfile .
podman run -d --name clear_urls_bot --pod clear_urls_bot_pod -p 3000:3000 --env-file .env clear_urls_bot
```

## 🚀 Funzionalità Avanzate

### 🛡️ Security Scanning

- **VirusTotal Security**: Automatic malware detection before URL cleaning ([docs](docs/VIRUSTOTAL.md))
  - Real-time scanning with 70+ antivirus engines
  - Detailed threat alerts with detection statistics
  - Alert-only mode (default) - notifications only for threats
  - Free tier: 4 requests/minute, 500/day, 15,500/month
  
- **URLScan.io Analysis**: Behavioral web reputation scanning ([docs](docs/URLSCAN.md))
  - Sandbox-based page analysis with screenshot capture
  - Risk scoring (0-100) and malicious classification
  - Private scans - your URLs stay confidential
  - Alert-only mode (default) - notifications only for threats
  - Phishing and dynamic content detection

### 📊 Statistics & Administration

- Statistiche globali e ranking utenti: /topusers, /toplinks
- Supporto multi-lingua: /language, /setlang <codice>
- Modalità privacy: /privacy per attivare/disattivare salvataggio cronologia
- Logging avanzato: solo admin riceve log critici via Telegram
- Notifiche automatiche errori: messaggio all'admin in caso di panic/errori

### 🔧 Performance & Reliability

- Backup automatico DB: script backup_db.sh, cron consigliato
- Caching risultati pulizia: cache interna per URL ripetuti
- Ottimizzazione DB/async: query asincrone, pooling, batch
- Webhook HTTPS: supportato nativamente, attivabile con `WEBHOOK_URL` + `WEBHOOK_SECRET`
- Comando `/redirect <url>`: restituisce frontend alternativi (Invidious, Piped, Nitter, Teddit, …) recuperati da LibRedirect e Farside, con cache TTL 6h

## 🛡️ Sicurezza e Best Practice

- Rate limiting anti-flood: massimo 1 richiesta/secondo per utente
- Validazione e sanificazione input su tutti i messaggi/callback
- Controllo permessi sistematico per azioni admin
- Protezione dati sensibili nei log e nelle variabili di ambiente
- Consigliato eseguire il bot in container rootless (Podman) e usare database PostgreSQL in produzione
- Backup automatico DB: script backup_db.sh, cron consigliato
- Logging avanzato: solo admin riceve log critici via Telegram
- Notifiche automatiche errori: messaggio all’admin in caso di panic/errori
- Caching risultati pulizia: cache interna per URL ripetuti
- Ottimizzazione DB/async: query asincrone, pooling, batch
- Webhook HTTPS: supportato nativamente, attivabile con `WEBHOOK_URL` + `WEBHOOK_SECRET`
- Integrazione VirusTotal: controllo link sospetti, avviso all’utente

## 🔒 Sicurezza

- Rate limiting anti-flood: massimo 1 richiesta/secondo per utente
- Validazione e sanificazione input su tutti i messaggi/callback
- Controllo permessi sistematico per azioni admin
- Protezione dati sensibili nei log e nelle variabili di ambiente
- Consigliato eseguire il bot in container rootless (Podman) e usare database PostgreSQL in produzione

## 🏗️ Technical Architecture

### Core Technologies

- **Language**: Rust 2021 Edition (MSRV 1.75+, tested on 1.92)
- **Bot Framework**: Teloxide 0.17 with modern async patterns
- **Database**: sqlx 0.8 with SQLite/PostgreSQL support
- **Caching**: Moka 0.12 for high-performance caching
- **Observability**: Comprehensive tracing with structured logging

### Performance Optimizations

- **Build**: Optimized LTO, single codegen unit, panic=abort for releases
- **Runtime**: Async I/O, connection pooling, efficient caching strategies
- **Memory**: Zero-copy patterns where possible, minimal allocations

### Security Features

- **Containerless**: Rootless Podman operation by default
- **Least Privilege**: Non-root container execution
- **Secure Defaults**: TLS-only, secure cookie handling, input validation

## 🔒 Security Best Practices

- Tutti gli input utente sono validati e sanificati lato bot.
- Rate limiting anti-flood: massimo 1 richiesta/secondo per utente.
- Le azioni amministrative sono protette da controllo su `ADMIN_ID`.
- Nessun dato sensibile (token, chiavi, dati personali) viene mai loggato.
- Le variabili di ambiente `.env` devono avere permessi restrittivi (`chmod 600 .env`).
- I log oscurano dati sensibili tramite redazione automatica.
- Consigliato eseguire il bot in container rootless (Podman) e usare database PostgreSQL in produzione.

## 🔧 Development

```bash
# Install dependencies
cargo build

# Run tests
cargo test

# Check code quality
cargo clippy --all-targets
cargo fmt --check

# Build release (optimized)
cargo build --release

# Local development with auto-reload
cargo install cargo-watch
cargo watch -x run
```

## 📊 Monitoring & Observability

The bot includes comprehensive observability:

```bash
# View logs
podman logs -f clear_urls_bot

# Check container status
./podman-deploy.sh status

# Monitor resource usage
podman stats clear_urls_bot
```

## 🐳 Container Details

- **Base Image**: debian:bookworm-slim (production)
- **Multi-stage**: Optimized build with minimal runtime footprint
- **Size**: ~80MB compressed, ~200MB uncompressed
- **Security**: Non-root user, SELinux labeling, read-only filesystem where possible

## 📦 Deployment & Backup

- Backup automatico DB: script backup_db.sh, cron consigliato
- Esempio cron:
  - 0 2 ** * /workspaces/clearurlsbot/backup_db.sh
- Oppure manuale: ./backup_db.sh

## 📝 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

**Note**: This bot has undergone significant modernization with improved performance, security, and maintainability. See the [CHANGELOG](CHANGELOG.md) for detailed updates.

## 🌍 Multi-lingua

- Comando /language per mostrare lingue disponibili
- Comando /setlang <codice> per cambiare lingua
- Struttura pronta per aggiungere altre lingue
