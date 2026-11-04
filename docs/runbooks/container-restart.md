# Runbook: Container Unexpected Restart

**Alert:** `URLCleanseBotContainerRestart` (from `monitoring/alerts.yml`)  
**Severity:** Warning  
**SLO Impact:** Availability (brief downtime during restart)  
**Owner:** Backend On-call / Platform

---

## Symptoms

- `docker ps` shows container age < 1m with `Restarting` or recent `Up X minutes`
- `increase(urlcleansebot_container_restarts_total[5m]) > 0`
- No OOM kill event (check `container_oom_events_total`)
- Health check failures preceding restart
- Telegram updates missed during restart window
- Logs show: `Starting URLCleanseBot` repeatedly

---

## Diagnosis

### 1. Check restart reason
```bash
# Container inspect
docker inspect url_cleanse_bot | jq '.[0].State | {ExitCode, OOMKilled, RestartCount, FinishedAt, StartedAt}'

# Exit code meanings:
# 0   = clean exit (SIGTERM)
# 137 = SIGKILL (OOM or docker stop -t 0)
# 1   = application error/panic
# 139 = SIGSEGV (segfault)
```

### 2. Check logs before restart
```bash
# Previous container logs (if docker logs retained)
docker logs url_cleanse_bot --tail 200

# If logs rotated, check journald
journalctl -u docker -n 200 | grep url_cleanse_bot

# Check for panic/backtrace
docker logs url_cleanse_bot --tail 500 | grep -A 20 "panicked\|thread.*panicked\|SIGSEGV\|segmentation fault"
```

### 3. Check health check failures
```bash
# Health check status
docker inspect url_cleanse_bot | jq '.[0].State.Health'

# Health check command
docker inspect url_cleanse_bot | jq '.[0].Config.Healthcheck'

# Manual health check
docker exec url_cleanse_bot /app/bot --health
curl -f http://localhost:3000/healthz
```

### 4. Check deploy / config changes
```bash
# Recent docker events
docker events --since 1h --filter container=url_cleanse_bot

# Check if image changed
docker images --filter reference=url_cleanse_bot

# Check compose file mtime
stat /home/giuliano/Scrivania/progetti/urlcleansebot/compose.yml
```

---

## Mitigation (Immediate Workaround)

### 1. If health check too aggressive
```bash
# Edit Dockerfile: increase start-period or interval
# HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=5
docker compose build url_cleanse_bot && docker compose up -d
```

### 2. If panic on startup (config/migration)
```bash
# Check config validation
docker run --rm --env-file .env url_cleanse_bot --help
# Or check logs for "Invalid configuration" / "Configuration validation failed"

# Fix .env, then restart
docker compose up -d
```

### 3. Disable auto-restart temporarily (for debugging)
```bash
# Edit compose.yml: restart: "no"
docker compose up -d
# Investigate, then re-enable: restart: unless-stopped
```

---

## Resolution (Root Cause Fix)

| Exit Code | Cause | Fix |
|-----------|-------|-----|
| **137 (SIGKILL)** | OOM kill | See `oom-kill.md` runbook |
| **137 (SIGKILL)** | `docker stop` / deploy | Normal during deploy; ensure graceful shutdown handles SIGTERM |
| **1** | Config error | Fix `.env`; validate `Config::from_env()` |
| **1** | Migration failed | Run `sqlx migrate run` manually; fix migration SQL |
| **1** | DB connection failed | Check DB reachable; fix `DATABASE_URL` |
| **1** | Panic in main | Fix unwrap/expect; add proper error handling |
| **139 (SIGSEGV)** | Rust UB / FFI bug | Enable `RUST_BACKTRACE=1`; use `cargo miri`; update deps |
| **0** | Clean shutdown | Normal; check why systemd/docker restarted it |

### Code improvements
- **Graceful shutdown**: Handle `SIGTERM` in `main.rs`; drain in-flight requests (30s timeout)
- **Health check**: Separate liveness (process alive) vs readiness (DB, AI, Telegram reachable)
- **Startup probe**: Longer `start-period` for cache warmup, rule fetch, DB migration
- **Panic hook**: Install `panic_hook` to log to structured logging before exit

---

## Rollback Procedure

```bash
# 1. Stop restart loop
docker stop url_cleanse_bot

# 2. Revert to previous image
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest

# 3. Revert config if changed
git -C /home/giuliano/Scrivania/progetti/urlcleansebot diff .env compose.yml

# 4. Start
docker compose up -d

# 5. Verify stable (no restart for 10m)
watch -n 30 'docker ps --filter name=url_cleanse_bot --format "{{.Status}}"'
```

---

## Prevention

1. **Health check hierarchy**: Liveness (process) vs Readiness (dependencies)
2. **Graceful shutdown**: `tokio::signal::ctrl_c()` → stop accepting → drain → exit
3. **Pre-flight checks**: Validate config, DB migration, rule fetch at startup before accepting traffic
4. **Deploy safety**: Blue/green or rolling with `max_surge: 1, max_unavailable: 0`
5. **Alert on restart rate**: `rate(container_restarts_total[5m]) > 0.1`
6. **Postmortem every unexpected restart**: Document root cause, add test/guard

---

## Links

- [Dockerfile healthcheck](Dockerfile#L37-L38)
- [Compose restart policy](compose.yml#L7)
- [Main.rs startup](src/main.rs#L10-L35)
- [OOM kill runbook](docs/runbooks/oom-kill.md)
- [High memory runbook](docs/runbooks/high-memory.md)
- [Alerts: ContainerRestart](monitoring/alerts.yml#L300-L313)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)