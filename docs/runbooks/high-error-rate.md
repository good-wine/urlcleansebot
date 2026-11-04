# Runbook: High Error Rate (General: Telegram API, Sanitization, DB, AI)

**Alert:** `URLCleanseBotHighErrorRate` (from `monitoring/alerts.yml`)  
**Severity:** Critical  
**SLO Impact:** Availability (>5% errors over 5m)  
**Owner:** Backend On-call

---

## Symptoms

- `urlcleansebot_requests_total{status=~"5..|timeout"}` rate > 5% of total requests
- Burn-rate alerts firing: `URLCleanseBotFastBurnRate` / `MediumBurnRate`
- Bot returns errors to users (inline: "error", messages: no reply)
- Health check may still pass (only sanitization path affected)
- Logs show mix of: Telegram 429/5xx, DB connection errors, AI timeouts, sanitization panics

---

## Diagnosis

### 1. Identify error source breakdown
```bash
# Error rate by status code
curl -s "http://localhost:9090/api/v1/query?query=sum by (status) (rate(urlcleansebot_requests_total{status=~'5..|timeout'}[5m]))"

# Error rate by handler type (message, inline, callback)
curl -s "http://localhost:9090/api/v1/query?query=sum by (handler) (rate(urlcleansebot_requests_total{status=~'5..|timeout'}[5m]))"

# Telegram API errors specifically
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_telegram_api_errors_total[5m])"

# DB errors
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_db_errors_total[5m])"

# AI errors
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_ai_requests_total{status=~'error|timeout'}[5m])"
```

### 2. Check recent logs for error patterns
```bash
# All errors in last 500 lines
docker logs url_cleanse_bot --tail 500 | grep -i "error\|failed\|panic\|timeout" | head -50

# Group by error type
docker logs url_cleanse_bot --tail 1000 | grep -i "error\|failed" | sed 's/.*error[:\s]*//i' | sort | uniq -c | sort -rn | head -20
```

### 3. Check component health
```bash
# Database
docker exec url_cleanse_bot sqlite3 /app/bot.db "SELECT 1;"  # or psql for PG

# AI API
export AI_API_KEY=$(grep AI_API_KEY .env | cut -d= -f2)
curl -s -H "Authorization: Bearer $AI_API_KEY" "https://api.openai.com/v1/models" | head -5

# Telegram API
curl -s "https://api.telegram.org/bot$(grep BOT_TOKEN .env | cut -d= -f2)/getMe" | jq

# Rule fetch
curl -s "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" | head -5
```

---

## Mitigation (Immediate Workaround)

### 1. Disable AI sanitization (if AI errors dominant)
```bash
# Edit .env: AI_API_KEY=
docker compose up -d
```

### 2. Enable aggressive fallback mode
```bash
# If rule engine failing, bot still works with basic cleaning
# No config change needed - graceful degradation is automatic
```

### 3. Restart container (clears transient state)
```bash
docker restart url_cleanse_bot
# Wait 10s for health check
curl -f http://localhost:3000/healthz
```

### 4. Switch to webhook with drop_pending_updates (if Telegram flood)
```bash
# In .env: WEBHOOK_URL=..., WEBHOOK_SECRET=...
# In src/main.rs: drop_pending_updates: true
docker compose up -d
```

---

## Resolution (Root Cause Fix)

| Error Source | Indicators | Fix |
|--------------|------------|-----|
| **Telegram 429/5xx** | `telegram_api_errors_total` high | Rate limiting middleware, webhook mode, circuit breaker |
| **DB connection/timeout** | `db_errors_total`, `db_pool_pending` > 0 | Increase pool size, fix connection leaks, add retries |
| **AI timeout/5xx** | `ai_requests_total{status=~"error|timeout"}` high | Stricter timeout (3s), caching, circuit breaker, disable AI |
| **Sanitization panic** | `panic` in logs, `sanitization_failures_total` | Fix regex, add bounds checks, fuzz test rule engine |
| **Rule fetch failure** | `rule_fetch_total{status="error"}` high | CDN fallback, ETag caching, local rule bundle |

### Code improvements
- **Error taxonomy**: Structured error types with `thiserror`; tag metrics with `error_type`
- **Circuit breakers**: Per-dependency (Telegram, AI, DB, Rules); fail fast, fallback
- **Retry budgets**: Max 3 retries with exponential backoff + jitter per request
- **Graceful degradation**: Priority: basic cleaning > AI > aggressive > Linkumori

---

## Rollback Procedure

```bash
# 1. Stop bot
docker stop url_cleanse_bot

# 2. Revert to previous stable image
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest

# 3. Restore DB if migration-related
# cp backups/bot.db.<timestamp> bot.db

# 4. Start
docker compose up -d

# 5. Verify error rate < 1%
curl -s "http://localhost:9090/api/v1/query?query=sum(rate(urlcleansebot_requests_total{status=~'5..|timeout'}[5m]))/sum(rate(urlcleansebot_requests_total[5m]))"
```

---

## Prevention

1. **Error budget alerts**: Burn-rate multi-window (already configured)
2. **Canary deploys**: 5% traffic to new version; auto-rollback on error rate > 2%
3. **Chaos testing**: Monthly: kill DB, throttle AI, simulate Telegram 429
4. **Fuzz testing**: `cargo fuzz` on rule engine, URL parser
5. **Dependency health checks**: Startup probes for DB, AI, Telegram, Rules
6. **Runbook drills**: Quarterly incident simulation with on-call

---

## Links

- [Burn-rate alerts](monitoring/alerts.yml#L8-L81)
- [Symptom alerts](monitoring/alerts.yml#L89-L180)
- [Prometheus metrics](src/metrics.rs)
- [Database runbook](docs/runbooks/db-pool-exhausted.md)
- [AI runbook](docs/runbooks/ai-high-error-rate.md)
- [Rule fetch runbook](docs/runbooks/rule-fetch-failure.md)
- [Container runbook](docs/runbooks/oom-kill.md)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)