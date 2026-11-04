# Runbook: Database Down / Unreachable

**Alert:** `URLCleanseBotDBPoolExhausted` (from `monitoring/alerts.yml`)  
**Severity:** Critical  
**SLO Impact:** Availability, Latency  
**Owner:** Backend On-call

---

## Symptoms

- Bot fails to start: `Database initialization error` in logs
- Health check endpoint returns 503 or times out
- `urlcleansebot_db_pool_connections_active` drops to 0
- `urlcleansebot_db_query_duration_seconds` spikes or shows NaN
- SQLite: `database is locked` / `unable to open database file`
- PostgreSQL: `connection refused` / `too many connections` / `pool exhausted`
- Migration errors: `Migration failed: ...` at startup

---

## Diagnosis

### 1. Check container and DB process
```bash
# Container status
docker ps -a --filter name=url_cleanse_bot

# Recent logs (last 200 lines)
docker logs url_cleanse_bot --tail 200

# Follow logs in real time
docker logs url_cleanse_bot -f
```

### 2. SQLite diagnostics (default)
```bash
# Check DB file exists and permissions
ls -la /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db

# Check for lock files
ls -la /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db-*

# Test SQLite connectivity
sqlite3 /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db ".tables"

# Check for long-running transactions / locks
sqlite3 /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db "PRAGMA lock_status;"
```

### 3. PostgreSQL diagnostics (if configured)
```bash
# Check PG container if separate
docker ps -a --filter name=postgres

# Test connectivity from bot container
docker exec url_cleanse_bot sh -c 'apt-get update && apt-get install -y postgresql-client && psql "$DATABASE_URL" -c "SELECT 1;"'

# Check connection count
docker exec postgres psql -U postgres -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'urlcleansebot';"

# Check pool exhaustion
docker exec postgres psql -U postgres -c "SELECT max_conn, used, max_conn - used as free FROM (SELECT setting::int as max_conn FROM pg_settings WHERE name = 'max_connections') m, (SELECT count(*) as used FROM pg_stat_activity) u;"

# Check for blocked queries
docker exec postgres psql -U postgres -c "SELECT pid, state, wait_event_type, wait_event, query_start, query FROM pg_stat_activity WHERE state != 'idle';"
```

### 4. Connection pool metrics (Prometheus)
```bash
# Query current pool state
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_db_pool_connections_active"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_db_pool_connections_idle"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_db_pool_connections_pending"
```

### 5. Migration status
```bash
# Check applied migrations
sqlite3 /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db "SELECT * FROM _sqlx_migrations ORDER BY run_on DESC LIMIT 10;"

# Or for PostgreSQL
docker exec postgres psql -U postgres -d urlcleansebot -c "SELECT * FROM _sqlx_migrations ORDER BY run_on DESC LIMIT 10;"

# Trace DB query spans (OpenTelemetry)
otelcli span list --service urlcleansebot --name "db.query" --endpoint http://localhost:4318/v1/traces | head -20
```

---

## Mitigation (Immediate Workaround)

### SQLite: Lock contention
```bash
# Remove stale lock files (bot must be stopped)
docker stop url_cleanse_bot
rm -f /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db-*
docker start url_cleanse_bot
```

### SQLite: Corrupted DB (last resort)
```bash
docker stop url_cleanse_bot
mv /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db.corrupted.$(date +%s)
docker start url_cleanse_bot  # Auto-creates fresh DB with schema
# Note: User configs and history lost. Restore from backup if needed.
```

### PostgreSQL: Pool exhausted
```bash
# Increase max_connections temporarily (requires PG restart)
docker exec postgres psql -U postgres -c "ALTER SYSTEM SET max_connections = 200;"
docker restart postgres

# Or kill idle connections
docker exec postgres psql -U postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE state = 'idle' AND state_change < now() - interval '10 minutes' AND datname = 'urlcleansebot';"
```

### PostgreSQL: Connection refused
```bash
# Check PG is running
docker ps --filter name=postgres

# Restart PG container
docker restart postgres

# Verify network connectivity
docker exec url_cleanse_bot ping postgres  # if using Docker network
```

---

## Resolution (Root Cause Fix)

| Cause | Fix |
|-------|-----|
| SQLite lock: concurrent access from backup script | Ensure `backup_db.sh` uses `sqlite3 .backup` or stops bot briefly; add file locking |
| SQLite lock: WAL mode not enabled | Add `?mode=rwc&journal_mode=WAL` to `DATABASE_URL` in `.env` |
| PG pool exhausted: max_connections too low | Increase `max_connections` in PG config; tune pool size in `src/db/implementation.rs` (currently 5) |
| PG pool exhausted: connection leak | Audit code for `Db::pool.acquire()` not dropped; enable `sqlx` query logging |
| Migration failed: schema drift | Run `sqlx migrate run --database-url "$DATABASE_URL"` manually; fix migration SQL |
| Migration failed: partial apply | Restore DB from backup, fix migration, re-apply |

### Code fixes to consider
- **Pool sizing**: In `src/db/implementation.rs`, adjust `max_connections` based on workload (currently 5)
- **Idle timeout**: Reduce `idle_timeout` from 600s if connections leak
- **Acquire timeout**: Increase `acquire_timeout` from 10s under high load
- **Health check**: Add `/healthz` DB ping in `src/main.rs` for k8s/liveness probe

---

## Rollback Procedure

```bash
# 1. Stop bot
docker stop url_cleanse_bot

# 2. Restore DB from latest backup
# Find latest backup
ls -la /home/giuliano/Scrivania/progetti/urlcleansebot/backups/bot.db.*

# Restore (example)
cp /home/giuliano/Scrivania/progetti/urlcleansebot/backups/bot.db.20260115_030000 /home/giuliano/Scrivania/progetti/urlcleansebot/bot.db

# 3. If migration caused issue, revert image
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest

# 4. Start bot
docker start url_cleanse_bot

# 5. Verify
docker logs url_cleanse_bot --tail 50
curl -f http://localhost:3000/healthz
```

---

## Prevention

1. **Automated backups**: Ensure `crontab.example` runs `backup_db.sh` daily; verify restores quarterly
2. **WAL mode for SQLite**: Set `DATABASE_URL=sqlite:bot.db?mode=rwc&journal_mode=WAL&busy_timeout=5000`
3. **Pool monitoring**: Alert on `urlcleansebot_db_pool_connections_pending > 0` for > 1m
4. **Connection leak detection**: Enable `sqlx` slow query log; add `tracing` spans around DB calls
5. **Migration safety**: Test migrations in staging; use `sqlx migrate run --dry-run` in CI
6. **Capacity planning**: Monitor `urlcleansebot_db_query_duration_seconds` p95; scale PG vertically before pool exhaustion

---

## Links

- [Database implementation](src/db/implementation.rs)
- [Config schema](src/config.rs#L23)
- [Docker Compose](compose.yml#L11)
- [Backup script](backup_db.sh)
- [Prometheus metrics](src/metrics.rs)
- [Alerts: Database](monitoring/alerts.yml) (search `URLCleanseBotDatabaseUnhealthy`)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)