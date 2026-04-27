# Deployment Guide 🚀

This comprehensive guide covers all deployment scenarios for ClearURLs Bot, from local development to production-grade deployments using Podman.

## 📋 Table of Contents

- [Prerequisites](#prerequisites)
- [Environment Configuration](#environment-configuration)
- [Local Development](#local-development)
- [Container Deployment](#container-deployment)
- [Production Deployment](#production-deployment)
- [Monitoring & Maintenance](#monitoring--maintenance)
- [Troubleshooting](#troubleshooting)

## 🔧 Prerequisites

### System Requirements

**Minimum:**

- CPU: 1 core
- RAM: 512MB
- Storage: 1GB
- OS: Linux (Ubuntu 20.04+, Debian 11+, CentOS 8+)

**Recommended:**

- CPU: 2 cores
- RAM: 1GB
- Storage: 5GB
- OS: Linux with systemd

### Software Requirements

**Core:**

- [Rust](https://www.rust-lang.org/tools/install) 1.85+ (MSRV)
- [Podman](https://podman.io/getting-started/installation) 3.0+

**Optional:**

- [Podman Compose](https://github.com/containers/podman-compose) 1.0+
- [PostgreSQL](https://www.postgresql.org/download/) 12+ (for production)
- [Nginx](https://nginx.org/en/install.html) (for reverse proxy)

### Novità

- Gestione errori migliorata e logging avanzato
- Modularità estesa (validazione, sanitizzazione, internazionalizzazione)
- Test automatizzati e cache per performance
- Internazionalizzazione dinamica via file JSON

## ⚙️ Environment Configuration

### Required Environment Variables

Create a `.env` file from `.env.example`:

```bash
# Copy the example file
cp .env.example .env
```

#### Core Configuration

```bash
# Telegram Bot Configuration
TELOXIDE_TOKEN=123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11
BOT_USERNAME=@your_bot_username
ADMIN_ID=123456789

# Optional webhook bind address (override default 0.0.0.0:8080)
#SERVER_ADDR=0.0.0.0:8080

# Application
APP_ENV=production
RUST_LOG=clear_urls_bot=info
DATABASE_URL=sqlite:bot.db
```

#### Optional AI Configuration

```bash
# AI Deep Scan (Optional)
AI_API_KEY=sk-your-openai-api-key
AI_API_BASE=https://api.openai.com/v1
AI_MODEL=gpt-4
```

#### Optional VirusTotal Configuration

```bash
# VirusTotal Malware Detection (Optional)
# Get your free API key at: https://www.virustotal.com/gui/my-apikey
# Free tier: 4 requests/minute, 500/day
# See docs/VIRUSTOTAL.md for complete setup guide
VIRUSTOTAL_API_KEY=your_virustotal_api_key_here
```

#### Webhook Mode (Optional)

By default the bot uses **long-polling** (one outbound persistent connection to
`api.telegram.org`). To switch to **webhooks** — required if you want zero idle
load, scale-to-zero on serverless platforms, or place the bot behind an HTTPS
load balancer — set the following variables:

```bash
# Public HTTPS URL Telegram will POST updates to.
# Must be a valid HTTPS endpoint reachable from the public internet.
WEBHOOK_URL=https://clear-urls-bot.example.com/webhook

# Random secret used to verify the X-Telegram-Bot-Api-Secret-Token header.
# Generate with one of:
#   openssl rand -hex 32
#   python3 -c "import secrets; print(secrets.token_hex(32))"
# Constraints: 16-256 chars, only A-Z a-z 0-9 _ -
WEBHOOK_SECRET=7f3a9c2e5b4d8f1e6a0c3b9d2e5f8a1c4b7d0e3f6a9c2e5b8d1f4a7c0e3b6d9f

# Port the embedded HTTP server binds to (default 8080).
PORT=8080
```

**How it works:**

- On startup, the bot calls `setWebhook` on the Telegram API automatically and
  starts an HTTP listener on `0.0.0.0:$PORT`.
- Every incoming POST is verified against `WEBHOOK_SECRET`; mismatched requests
  are rejected with `401`.
- When `WEBHOOK_URL` is **unset**, the bot falls back to long-polling (no HTTP
  server is started, no port is opened).

**Manual webhook registration** (only needed for debugging):

```bash
curl -X POST "https://api.telegram.org/bot$TELOXIDE_TOKEN/setWebhook" \
  -d "url=$WEBHOOK_URL" \
  -d "secret_token=$WEBHOOK_SECRET" \
  -d "max_connections=40" \
  -d "allowed_updates=[\"message\",\"edited_message\",\"callback_query\",\"inline_query\",\"chosen_inline_result\"]"

# Inspect current webhook
curl "https://api.telegram.org/bot$TELOXIDE_TOKEN/getWebhookInfo"

# Remove webhook (revert to long-polling)
curl -X POST "https://api.telegram.org/bot$TELOXIDE_TOKEN/deleteWebhook"
```

**When to choose which:**

| Scenario | Mode |
|---|---|
| Local development | Long-polling |
| Single small VPS / Replit / Container | Long-polling (simpler) |
| Leapcell Container with public domain | Either |
| Leapcell Functions / scale-to-zero | **Webhook** (required) |
| High-throughput bot behind LB | **Webhook** |

#### Database Options

**SQLite (Default):**

```bash
DATABASE_URL=sqlite:bot.db
```

**PostgreSQL (Recommended for Production):**

```bash
DATABASE_URL=postgresql://username:password@localhost:5432/clearurls_bot
```

### Security Best Practices

1. **Generate Secure Cookie Key**:

   ```bash
   openssl rand -hex 32  # Generate 32-character hex string
   ```

2. **File Permissions**:

   ```bash
   chmod 600 .env  # Restrict to owner only
   ```

3. **Environment-Specific Configs**:

   ```bash
   # Development
   cp .env.example .env.dev
   
   # Production
   cp .env.example .env.prod
   ```

### Security Best Practices

1. Tutti gli input utente sono validati e sanificati lato bot.
2. Rate limiting anti-flood: massimo 1 richiesta/secondo per utente.
3. Le azioni amministrative sono protette da controllo su `ADMIN_ID`.
4. Nessun dato sensibile (token, chiavi, dati personali) viene mai loggato.
5. Le variabili di ambiente `.env` devono avere permessi restrittivi (`chmod 600 .env`).
6. I log oscurano dati sensibili tramite redazione automatica.
7. Consigliato eseguire il bot in container rootless (Podman) e usare database PostgreSQL in produzione.

## 🛡️ Sicurezza e Best Practice

- **VirusTotal Security**: 🆕 Real-time malware detection with 70+ antivirus engines (see [docs/VIRUSTOTAL.md](VIRUSTOTAL.md))
- Rate limiting anti-flood: massimo 1 richiesta/secondo per utente
- Validazione e sanificazione input su tutti i messaggi/callback
- Controllo permessi sistematico per azioni admin
- Protezione dati sensibili nei log e nelle variabili di ambiente
- Consigliato eseguire il bot in container rootless (Podman) e usare database PostgreSQL in produzione
- Backup automatico DB: script backup_db.sh, cron consigliato
- Logging avanzato: solo admin riceve log critici via Telegram
- Notifiche automatiche errori: messaggio all'admin in caso di panic/errori
- Caching risultati pulizia: cache interna per URL ripetuti
- Ottimizzazione DB/async: query asincrone, pooling, batch
- Webhook HTTPS: pronto per refactor, supporto via env

## 🏠 Local Development

### Quick Start

```bash
# Clone repository
git clone https://github.com/yourusername/clear_urls_bot.git
cd clear_urls_bot

# Set up environment
cp .env.example .env
# Edit .env with your settings

# Install dependencies and run
cargo run
```

### Development Tools

```bash
# Install development dependencies
cargo install cargo-watch cargo-audit

# Run with auto-reload
cargo watch -x run

# Run tests
cargo test

# Check code quality
cargo clippy --all-targets
cargo fmt
```

### Database Setup (Local PostgreSQL)

```bash
# Install PostgreSQL
sudo apt install postgresql postgresql-contrib

# Create database and user
sudo -u postgres psql
CREATE DATABASE clearurls_dev;
CREATE USER clearurls_dev WITH PASSWORD 'your_password';
GRANT ALL PRIVILEGES ON DATABASE clearurls_dev TO clearurls_dev;
\q

# Set environment variable
export DATABASE_URL=postgresql://clearurls_dev:your_password@localhost/clearurls_dev
```

## 🐳 Container Deployment

### Option 1: Deployment Script (Recommended)

The included `podman-deploy.sh` script handles all deployment tasks:

```bash
# Make script executable
chmod +x podman-deploy.sh

# Build and run
./podman-deploy.sh start

# View logs
./podman-deploy.sh logs

# Check status
./podman-deploy.sh status

# Stop and clean up
./podman-deploy.sh stop
```

### Option 2: Podman Compose

```bash
# Start services
podman-compose -f podman-compose.yml up

# Detached mode
podman-compose -f podman-compose.yml up -d

# Stop services
podman-compose -f podman-compose.yml down

# View logs
podman-compose -f podman-compose.yml logs -f
```

### Option 3: Manual Podman Commands

```bash
# Build container
podman build -t clear_urls_bot -f Containerfile .

# Create pod for networking
podman pod create --name clear_urls_bot_pod -p 3000:3000

# Run container
podman run -d \
    --name clear_urls_bot \
    --pod clear_urls_bot_pod \
    --env-file .env \
    -v ./bot.db:/app/bot.db:Z \
    --memory=512m \
    --cpus=0.5 \
    --restart=unless-stopped \
    clear_urls_bot
```

### Volume Management

**For SQLite:**

```bash
# Create data directory
mkdir -p ./data
chmod 755 ./data

# Mount database file
-v ./data/bot.db:/app/bot.db:Z
```

**For PostgreSQL:**

```bash
# Run PostgreSQL container
podman run -d \
    --name postgres \
    -e POSTGRES_DB=clearurls_bot \
    -e POSTGRES_USER=clearurls \
    -e POSTGRES_PASSWORD=your_password \
    -v postgres_data:/var/lib/postgresql/data \
    postgres:15

# Connect bot to PostgreSQL
podman run -d \
    --name clear_urls_bot \
    --pod clear_urls_bot_pod \
    --env-file .env \
    --env DATABASE_URL=postgresql://clearurls:your_password@postgres:5432/clearurls_bot \
    clear_urls_bot
```

## 🏭 Production Deployment

### Production Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Load Balancer │───▶│  Reverse Proxy  │───▶│  ClearURLs Bot  │
│   (Optional)    │    │   (Nginx)       │    │   (Podman)      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                                        │
                                                        ▼
                                              ┌─────────────────┐
                                              │   Database      │
                                              │ (PostgreSQL)    │
                                              └─────────────────┘
```

### Systemd Service Setup

Create a systemd service for automatic startup:

```bash
# Create service file
sudo nano /etc/systemd/system/clearurls-bot.service
```

```ini
[Unit]
Description=ClearURLs Telegram Bot
After=network.target
Wants=network.target

[Service]
Type=forking
User=clearurls
Group=clearurls
WorkingDirectory=/opt/clearurls-bot
EnvironmentFile=/opt/clearurls-bot/.env
ExecStart=/usr/local/bin/podman-deploy.sh start
ExecStop=/usr/local/bin/podman-deploy.sh stop
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

```bash
# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable clearurls-bot
sudo systemctl start clearurls-bot
sudo systemctl status clearurls-bot
```

### Nginx Reverse Proxy

```nginx
# /etc/nginx/sites-available/clearurls-bot
server {
    listen 80;
    server_name your-domain.com;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

```bash
# Enable site
sudo ln -s /etc/nginx/sites-available/clearurls-bot /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### SSL/TLS Setup with Let's Encrypt

```bash
# Install certbot
sudo apt install certbot python3-certbot-nginx

# Get certificate
sudo certbot --nginx -d your-domain.com

# Auto-renewal
sudo systemctl enable certbot.timer
```

### Backup Strategy

**Database Backup Script:**

```bash
#!/bin/bash
# /opt/clearurls-bot/scripts/backup.sh

BACKUP_DIR="/opt/backups/clearurls-bot"
DATE=$(date +%Y%m%d_%H%M%S)
CONTAINER_NAME="postgres"

# Create backup directory
mkdir -p $BACKUP_DIR

# Database backup
podman exec $CONTAINER_NAME pg_dump -U clearurls clearurls_bot > "$BACKUP_DIR/db_backup_$DATE.sql"

# Compress backup
gzip "$BACKUP_DIR/db_backup_$DATE.sql"

# Remove old backups (keep 7 days)
find $BACKUP_DIR -name "db_backup_*.sql.gz" -mtime +7 -delete

echo "Backup completed: $BACKUP_DIR/db_backup_$DATE.sql.gz"
```

```bash
# Schedule daily backups
crontab -e
# Add line:
0 2 * * * /opt/clearurls-bot/scripts/backup.sh
```

## 📊 Monitoring & Maintenance

### Health Monitoring

```bash
# Container health check
podman exec clear_urls_bot curl -f http://localhost:3000/health || exit 1

# Add to Containerfile for built-in health checks
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1
```

### Log Management

```bash
# View logs
podman logs -f clear_urls_bot

# Log rotation (podman handles this automatically with compose)
# Manual log cleanup
podman system prune --volumes
```

### Performance Monitoring

```bash
# Resource usage
podman stats clear_urls_bot

# System resources
htop
iostat -x 1
```

### Updating the Application

```bash
# Automated update script
#!/bin/bash
# /opt/clearurls-bot/scripts/update.sh

cd /opt/clearurls-bot

# Pull latest changes
git pull origin main

# Rebuild container
podman rmi clear_urls_bot:latest || true
./podman-deploy.sh build

# Restart service
sudo systemctl restart clearurls-bot

echo "Update completed successfully!"
```

## 🐛 Troubleshooting

### Common Issues

#### Container Won't Start

```bash
# Check logs
podman logs clear_urls_bot

# Check configuration
podman exec clear_urls_bot env | grep -E "(DATABASE_URL|TELOXIDE_TOKEN)"

# Check if ports are in use
ss -tulpn | grep 3000
```

#### Database Connection Issues

```bash
# Test database connection
podman exec clear_urls_bot cargo check

# For PostgreSQL
podman exec postgres psql -U clearurls -d clearurls_bot -c "SELECT 1;"

# For SQLite
podman exec clear_urls_bot ls -la /app/bot.db
```

#### Permission Issues

```bash
# Fix file permissions
sudo chown -R clearurls:clearurls /opt/clearurls-bot
chmod +x /opt/clearurls-bot/podman-deploy.sh

# Fix SELinux contexts (if applicable)
sudo restorecon -R /opt/clearurls-bot
```

#### Memory Issues

```bash
# Check memory usage
podman stats --no-stream clear_urls_bot

# Increase memory limits
podman stop clear_urls_bot
podman run -d --memory=1g ... # Increase to 1GB
```

### Debug Mode

```bash
# Enable debug logging
export RUST_LOG=debug
export RUST_LOG_STYLE=always

# Restart with debug
podman stop clear_urls_bot
podman run -d \
    --name clear_urls_bot \
    --env-file .env \
    --env RUST_LOG=debug \
    --env RUST_LOG_STYLE=always \
    clear_urls_bot
```

### Recovery Procedures

#### Database Corruption

```bash
# SQLite recovery
cp bot.db bot.db.backup
sqlite3 bot.db ".recover" | sqlite3 bot_recovered.db
mv bot_recovered.db bot.db

# PostgreSQL recovery
podman exec postgres pg_dump -U clearurls clearurls_bot > backup.sql
# Drop and recreate database if needed
podman exec postgres psql -U clearurls -c "DROP DATABASE IF EXISTS clearurls_bot;"
podman exec postgres psql -U clearurls -c "CREATE DATABASE clearurls_bot;"
podman exec -i postgres psql -U clearurls clearurls_bot < backup.sql
```

#### Container Recovery

```bash
# Complete reset
podman stop clear_urls_bot
podman rm clear_urls_bot
podman pod rm clear_urls_bot_pod

# Fresh deployment
./podman-deploy.sh start
```

## 📞 Getting Help

- **Issues**: [GitHub Issues](https://github.com/yourusername/clear_urls_bot/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/clear_urls_bot/discussions)
- **Documentation**: [Full Documentation](docs/)
- **Architecture**: [Architecture Guide](ARCHITECTURE.md)

---

This deployment guide should cover all scenarios from development to production. For specific issues not covered here, please open an issue on GitHub.
