# Guida Rapida — ClearURLs Telegram Bot

Una guida passo-passo per configurare e avviare il bot in pochi minuti.

## Prerequisiti

- **Rust 1.88+** (scarica da [rustup.rs](https://rustup.rs/))
- **Podman** (consigliato) o Docker
- **Git**

### Telegram

1. Contatta [@BotFather](https://t.me/botfather) su Telegram
2. Crea un nuovo bot con `/newbot`
3. Salva il **token**

## Installazione Rapida

### 1. Clona e configura

```bash
git clone https://github.com/good-wine/clearurlsbot.git
cd clearurlsbot
cp .env.example .env
```

### 2. Configura `.env`

```ini
TELOXIDE_TOKEN=il_tuo_token
BOT_USERNAME=@il_tuo_bot
ADMIN_ID=il_tuo_user_id
```

### 3. Avvia

```bash
cargo run              # sviluppo
cargo run --release    # produzione
```

### 4. Test

1. Apri Telegram e cerca il tuo bot
2. Invia `/start`
3. Invia un URL con tracking: `https://example.com?utm_source=test&fbclid=123`
4. Il bot risponde con l'URL pulito

## Configurazione Avanzata

### Sicurezza

```ini
VIRUSTOTAL_API_KEY=la_tua_chiave
VIRUSTOTAL_ALERT_ONLY=true
URLSCAN_API_KEY=la_tua_chiave
URLSCAN_ALERT_ONLY=true
```

### AI Deep Scan

```ini
AI_API_KEY=la_tua_chiave_openai
AI_API_BASE=https://api.openai.com/v1
AI_MODEL=gpt-4
```

### Database

```ini
# SQLite (default)
DATABASE_URL=sqlite:bot.db

# PostgreSQL (produzione)
DATABASE_URL=postgres://user:password@localhost/clearurls
```

### Webhook

```ini
WEBHOOK_URL=https://il-tuo-dominio.com/webhook
WEBHOOK_SECRET=un_secret_lungo_e_casuale
PORT=8080
```

## Comandi Principali

| Comando | Descrizione |
|---------|-------------|
| `/start` | Inizializza il bot |
| `/help` | Mostra tutti i comandi |
| `/settings` | Menu impostazioni interattivo |
| `/stats` | Statistiche personali |
| `/history` | Ultimi 10 URL puliti |
| `/leaderboard` | Top 10 utenti |
| `/language` | Mostra lingue disponibili |
| `/setlang <code>` | Cambia lingua (it/en/es/fr/de/pt/ru/ar/hi/zh/ja/ko/tr/nl/pl) |

## Troubleshooting

### Il bot non risponde?

1. Verifica che `TELOXIDE_TOKEN` sia corretto
2. Controlla i log: `RUST_LOG=debug cargo run`
3. Assicurati che il bot sia online

### Errori di compilazione?

```bash
rustup update
cargo clean && cargo build
```

### Problemi con Podman?

```bash
podman --version
podman ps
./podman-deploy.sh logs
```

## Documentazione Completa

- **[README.md](README.md)** — Documentazione principale
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Architettura
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Architettura dettagliata
- **[docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)** — Guide di deployment
- **[LANGUAGES.md](LANGUAGES.md)** — Lingue supportate e guida traduzione
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — Come contribuire
- **[SECURITY.md](SECURITY.md)** — Policy di sicurezza

## Supporto

- **Issues**: [GitHub Issues](https://github.com/good-wine/clearurlsbot/issues)
- **Discussions**: [GitHub Discussions](https://github.com/good-wine/clearurlsbot/discussions)
