# 0004 - Deployment Model: Docker Compose Distroless Single-Instance

**Status**: Accepted

**Date**: 2026-11-04

## Context

URLCleanseBot targets self-hosted deployment on a single VPS. Users range from hobbyists to small teams. The deployment must be:
- Simple (single command)
- Secure (minimal attack surface)
- Resource-constrained (typical VPS: 1-2 vCPU, 1-2GB RAM)
- Observable (health checks, logs, metrics)
- Updatable (zero-downtime or minimal-downtime)

## Decision

**Deploy as a single Docker Compose service using a distroless runtime image, with health checks, resource limits, and volume-mounted database.**

### Docker Image Strategy

**Multi-stage build:**
1. **Builder stage** (`rust:1.88-slim`): Compile release binary with cached dependencies
2. **Runtime stage** (`gcr.io/distroless/cc-debian12:nonroot`): ~25 MB final image, no shell, no package manager, non-root user

### Docker Compose Configuration

```yaml
services:
  url_cleanse_bot:
    build: .
    container_name: url_cleanse_bot
    restart: unless-stopped
    ports:
      - "3000:3000"          # Webhook endpoint (if configured)
    volumes:
      - ./bot.db:/app/bot.db  # SQLite persistence
    env_file: .env
    environment:
      - APP_ENV=production
    deploy:
      resources:
        limits:
          memory: 512M
          cpus: '0.5'
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
    security_opt:
      - no-new-privileges:true
```

### Health Checks

- **Container-level**: `HEALTHCHECK` in Dockerfile calls `/app/bot --health` (starts HTTP server on `:3000/health`)
- **Interval**: 30s, timeout 3s, start period 10s, retries 3
- **Compose**: `restart: unless-stopped` handles crash recovery

### Networking Modes

1. **Long-polling (default)**: No inbound ports needed; bot initiates connections to Telegram
2. **Webhook mode**: Set `WEBHOOK_URL` + `WEBHOOK_SECRET`; port 3000 receives HTTPS POST from Telegram
   - Requires reverse proxy (nginx/Caddy) for TLS termination
   - `WEBHOOK_SECRET` validates `X-Telegram-Bot-Api-Secret-Token` header

### Update Strategy

**Rolling update via `docker compose up -d --build`:**
1. Build new image
2. Stop old container (SIGTERM → graceful shutdown: close DB pool, finish in-flight requests)
3. Start new container
4. Health check passes → traffic resumes

Downtime: ~5-10 seconds (single instance). Acceptable for self-hosted bot.

### Secrets Management

- **`.env` file**: `BOT_TOKEN`, `AI_API_KEY`, `WEBHOOK_SECRET`, `DATABASE_URL`
- **File permissions**: `chmod 600 .env`
- **No secrets in image**: All injected at runtime via `env_file`
- **Distroless**: No shell to inspect env at runtime

### Backup Strategy

- **Database**: `cp bot.db bot.db.$(date +%F).bak` (cron or manual)
- **Config**: `.env` file backup
- **Rules cache**: Re-fetched on startup (ClearURLs, LibRedirect, Farside)

## Consequences

### Positive
- **Minimal attack surface**: Distroless, non-root, no-new-privileges, no shell
- **Single-file deployment**: `docker compose up -d` — no k8s, no cluster
- **Resource predictable**: Hard limits prevent OOM kills
- **Fast startup**: ~1-2s (binary + DB init)
- **Portable**: Runs on any Docker host (x86_64, arm64)

### Negative
- **Single point of failure**: No HA (out of scope for self-hosted)
- **No horizontal scaling**: Webhook mode requires single endpoint
- **Manual updates**: No auto-update (by design; user controls timing)
- **Volume management**: User must ensure `./bot.db` persists across updates

## Alternatives Considered

1. **Kubernetes/Helm**: Rejected — overkill for single-instance VPS
2. **Systemd + bare binary**: Rejected — no isolation, harder dependency management
3. **Podman rootless**: Supported via `podman-deploy.sh` script; Compose works with Podman 4+
4. **Watchtower auto-update**: Rejected — uncontrolled restarts risk in-flight request loss

## Links

- [Dockerfile](../Dockerfile)
- [compose.yml](../compose.yml)
- [podman-deploy.sh](../podman-deploy.sh)
- [Distroless images](https://github.com/GoogleContainerTools/distroless)
- [Health check implementation](../src/main.rs) (search for `--health`)