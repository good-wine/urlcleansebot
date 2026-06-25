# Deployment Guide

## Prerequisites

### System Requirements

- **CPU**: 1 core (min), 2 cores (recommended)
- **RAM**: 512 MB (min), 1 GB (recommended)
- **Storage**: 1 GB
- **OS**: Linux (Ubuntu 20.04+, Debian 11+, Fedora 35+)

### Software

- **Rust 1.88+** (for native build) or **Podman 4+** / **Docker** (for containerized)
- **PostgreSQL 12+** (optional, for production)

## Environment Configuration

Create `.env` from `.env.example`:

```bash
cp .env.example .env
chmod 600 .env
```

### Core

```bash
TELOXIDE_TOKEN=123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11
BOT_USERNAME=@your_bot_username
ADMIN_ID=123456789

# Database (SQLite default, PostgreSQL for production)
DATABASE_URL=sqlite:bot.db
# DATABASE_URL=postgresql://user:password@localhost:5432/url_cleanse_bot

# Logging
RUST_LOG=url_cleanse_bot=info
```

### Optional Integrations

```bash
# AI Deep Scan
AI_API_KEY=sk-your-openai-api-key
AI_API_BASE=https://api.openai.com/v1
AI_MODEL=gpt-4
```

### Webhook Mode

By default the bot uses **long-polling** (persistent connection to `api.telegram.org`).
Switch to **webhooks** for scale-to-zero or HTTPS load balancing:

```bash
WEBHOOK_URL=https://your-domain.com/webhook
WEBHOOK_SECRET=$(openssl rand -hex 32)
PORT=8080
```

The bot automatically calls `setWebhook` on startup. Webhook requests are verified
against `WEBHOOK_SECRET` via the `X-Telegram-Bot-Api-Secret-Token` header.

## Deployment Options

### Option 1: Native (no container)

```bash
cargo build --release --locked
./target/release/url_cleanse_bot
```

### Option 2: Docker/Podman

```bash
# Build
podman build -t urlcleansebot -f Dockerfile .

# Run (SQLite)
podman run -d --name urlcleansebot --env-file .env -v ./bot.db:/app/bot.db:Z urlcleansebot

# Run (PostgreSQL)
podman run -d --name urlcleansebot --env-file .env urlcleansebot
```

The `Dockerfile` uses a multi-stage build:
- **Builder**: `rust:1.88-slim` — compiles with LTO + strip
- **Runtime**: `gcr.io/distroless/cc-debian12:nonroot` — ~25 MB, zero CVEs

### Option 3: Docker Compose

```bash
cp compose.yml .env  # configure your .env first
podman-compose up -d
# or
docker compose up -d
```

`compose.yml` includes the bot plus optional PostgreSQL and health checks.

### Option 4: Render.com

1. Fork the repository and connect it to Render
2. Use `runtime: rust` (free plan) — Render's buildpack auto-detects the Rust project
3. Set environment variables in Render dashboard:
   - `TELOXIDE_TOKEN`, `BOT_USERNAME`, `ADMIN_ID`
   - `WEBHOOK_URL`, `WEBHOOK_SECRET` (required on Render free tier for HTTPS termination)
4. Render will run `cargo build --release --locked` and start the binary

The bot exposes a health endpoint at `GET /health`.

## Health Monitoring

The bot exposes an HTTP health endpoint:

```bash
curl http://localhost:$PORT/health
# → 200 OK
```

## Metrics

Prometheus-format counters are available at `/metrics` on the same HTTP server:

```bash
curl http://localhost:$PORT/metrics
# → # HELP url_cleanse_bot_requests_total Total requests by type
# → url_cleanse_bot_requests_total{type="message"} 42
```

## Performance Tuning

### Database

- **SQLite**: Set `DATABASE_URL=sqlite:bot.db?mode=rwc` for WAL mode
- **PostgreSQL**: Tune `max_connections` and `shared_buffers` based on expected load

### Cache

The bot uses moka for in-process caching:
- URL cleaning results: 10k entries, 1h TTL
- Multi-source catalog: configurable TTL
- LibRedirect/Farside catalogs: 6h TTL

## Troubleshooting

### Container won't start

```bash
podman logs urlcleansebot
podman exec urlcleansebot env | grep -E "(DATABASE_URL|TELOXIDE_TOKEN)"
```

### Database issues

```bash
# SQLite
ls -la bot.db

# PostgreSQL
psql -U user -d url_cleanse_bot -c "SELECT 1;"
```

### Enable debug logging

```bash
RUST_LOG=debug cargo run
# or
podman run -e RUST_LOG=debug urlcleansebot
```
