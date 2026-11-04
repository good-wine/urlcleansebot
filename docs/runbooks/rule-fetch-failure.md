# Runbook: Rule Source Fetch Failure (ClearURLs, AdGuard, Brave, Farside, LibRedirect)

**Alert:** `URLCleanseBotRuleFetchFailing` (from `monitoring/alerts.yml`)  
**Severity:** Warning (fetch failure) → Critical (rules stale > 24h)  
**SLO Impact:** Quality (tracking params not removed), Latency (fallback to cache)  
**Owner:** Backend On-call

---

## Symptoms

- Logs: `ClearURLs rules download failed after retries: ...`
- `urlcleansebot_rule_fetch_duration_seconds` NaN or > 60s
- `urlcleansebot_rule_fetch_failures_total` increasing
- `urlcleansebot_rules_age_seconds` > 86400 (24h)
- `urlcleansebot_providers_loaded` = 0 or dropped significantly
- Bot starts but `RuleEngine.providers` empty → only cache/fallback works
- User-visible: known tracking params (utm_*, fbclid, gclid) not stripped

---

## Diagnosis

### 1. Check rule fetch logs
```bash
# Recent fetch attempts
docker logs url_cleanse_bot --tail 200 | grep -i "clearurls\|rule\|provider\|download\|fetch"

# Follow in real time
docker logs url_cleanse_bot -f | grep -i "rule"
```

### 2. Test each source URL directly
```bash
# ClearURLs (primary)
curl -s -w "\nHTTP: %{http_code}\nTime: %{time_total}s\nSize: %{size_download}\n" \
  "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" | head -5

# AdGuard
curl -s -w "\nHTTP: %{http_code}\n" \
  "https://raw.githubusercontent.com/AdguardTeam/AdGuardSDNSFilter/master/Filters/filter.txt" | head -5

# Brave
curl -s -w "\nHTTP: %{http_code}\n" \
  "https://raw.githubusercontent.com/brave/brave-browser/master/components/ad_block/resources/brave-unbreak-list.txt" | head -5

# Farside (LibRedirect instances)
curl -s -w "\nHTTP: %{http_code}\n" \
  "https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json" | head -5

# LibRedirect instances
curl -s -w "\nHTTP: %{http_code}\n" \
  "https://raw.githubusercontent.com/libredirect/instances/main/data.json" | head -5
```

### 3. Validate JSON schema (ClearURLs)
```bash
# Download and validate structure
curl -s "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" | \
  jq '.providers | length, keys[0:5]'

# Check for expected fields
curl -s "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" | \
  jq '.providers."google.com" | {urlPattern, rules: .rules[0:3], exceptions: .exceptions[0:3]}'
```

### 4. Check from inside container (network/DNS)
```bash
# Test DNS resolution
docker exec url_cleanse_bot getent hosts raw.githubusercontent.com

# Test connectivity with container's CA certs
docker exec url_cleanse_bot wget -qO- --timeout=10 \
  "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" | head -5

# Check proxy/env vars
docker exec url_cleanse_bot env | grep -i proxy
```

### 5. Metrics analysis
```bash
# Fetch success rate
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_rule_fetch_total[1h])"
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_rule_fetch_failures_total[1h])"

# Rules freshness
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rules_age_seconds"

# Providers loaded
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_providers_loaded"
```

### 6. Check compiled provider count in logs
```bash
docker logs url_cleanse_bot --tail 100 | grep -i "provider\|compiled\|loaded"
# Expected: "Compiled X providers from ClearURLs"
```

### 7. Trace rule fetch operations (OpenTelemetry)
```bash
# Get trace ID from logs
docker logs url_cleanse_bot --tail 100 | grep -i "trace_id\|span_id" | head -5

# Query rule fetch spans via otelcli
otelcli span list --service urlcleansebot --name "rule.fetch" --endpoint http://localhost:4318/v1/traces | head -20

# Query rule compilation spans
otelcli span list --service urlcleansebot --name "rule.compile" --endpoint http://localhost:4318/v1/traces | head -20
```

---

## Mitigation (Immediate Workaround)

### Use cached rules (automatic)
- Bot loads rules from memory cache on restart if fetch fails
- Cache TTL: 1 hour (see `RuleEngine::new_lazy` in `src/sanitizer/rule_engine/mod.rs`)
- Inline URL cache: 10k entries, no TTL (see `PIPELINE_URL_CACHE` in `src/sanitizer/pipeline.rs`)

### Force refresh with backup source
```bash
# If primary fails, try alternative ClearURLs mirror
# Edit .env: CLEARURLS_SOURCE=https://cdn.jsdelivr.net/gh/ClearURLs/Rules@master/data.min.json
docker compose up -d
```

### Manual rule injection (emergency)
```bash
# 1. Download rules locally
curl -s "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json" > /tmp/rules.json

# 2. Validate
jq '.providers | length' /tmp/rules.json

# 3. Inject via config map / volume mount (requires rebuild)
# Or restart bot with local file: CLEARURLS_SOURCE=file:///app/rules.json
# Mount: - ./rules.json:/app/rules.json in compose.yml
```

### Disable rule engine temporarily (last resort)
```bash
# Set empty source to skip fetch
docker exec url_cleanse_bot sh -c 'echo "CLEARURLS_SOURCE=" >> /app/.env'
docker restart url_cleanse_bot
# Only aggressive mode + custom rules + AI will work
```

---

## Resolution (Root Cause Fix)

| Cause | Fix |
|-------|-----|
| **Network timeout (30s)** | Increase timeout in `RuleEngine::refresh()` (currently 30s); add retry with backoff (already present) |
| **HTTP 404 / 410** | Source URL changed; update `CLEARURLS_SOURCE` in `.env` to new location |
| **Schema change** | ClearURLs JSON structure changed; update `compile_providers` in `src/sanitizer/rule_engine/clearurls.rs` |
| **Rate limited by GitHub** | Add `If-None-Match` / `ETag` support; use CDN (jsdelivr, raw.githack); cache for 24h |
| **DNS / TLS failure** | Check container CA certs; use IP direct; configure `/etc/hosts` in container |
| **Large response OOM** | Response > 50MB; stream parsing instead of loading full JSON |

### Code improvements
- **ETag/If-Modified-Since**: Add conditional requests in `RuleEngine::refresh()`
- **CDN fallback**: Try `cdn.jsdelivr.net/gh/ClearURLs/Rules@master/data.min.json` first
- **Streaming JSON parse**: Use `serde_json::StreamDeserializer` to avoid full load
- **Multiple sources**: Aggregate ClearURLs + AdGuard + Brave + custom rules
- **Schema versioning**: Detect schema version; log warning on mismatch

---

## Rollback Procedure

```bash
# 1. Revert to previous working image (rules compiled in)
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest
docker compose up -d

# 2. Or restore local rules file
cp /home/giuliano/Scrivania/progetti/urlcleansebot/backups/rules.json.20260115 /home/giuliano/Scrivania/progetti/urlcleansebot/rules.json
# Edit compose.yml to mount: - ./rules.json:/app/rules.json
# Set CLEARURLS_SOURCE=file:///app/rules.json
docker compose up -d

# 3. Verify providers loaded
docker logs url_cleanse_bot --tail 30 | grep -i "compiled.*provider"
curl -f http://localhost:3000/healthz
```

---

## Prevention

1. **Mirror sources**: Configure primary + 2 fallback URLs in config (ClearURLs, jsDelivr, raw.githack)
2. **Conditional fetch**: Implement ETag/Last-Modified; only download on change
3. **Local cache persistence**: Save rules to volume; load from disk on startup before fetch
4. **Schema validation**: Validate JSON structure before compiling; alert on schema drift
5. **Fetch scheduling**: Decouple fetch from startup; background job every 6h with jitter
6. **Alert on staleness**: `URLCleanseBotRulesStale` at 12h, critical at 24h
7. **CI test**: In CI, fetch and compile rules; fail build if schema breaks

---

## Links

- [Rule Engine implementation](src/sanitizer/rule_engine/mod.rs)
- [ClearURLs parser](src/sanitizer/rule_engine/clearurls.rs)
- [Config: CLEARURLS_SOURCE](src/config.rs#L26)
- [HTTP retry utility](src/http_utils.rs)
- [Prometheus metrics](src/metrics.rs) (search `rule_fetch`, `providers_loaded`)
- [Alerts: Rule Fetch](monitoring/alerts.yml) (search `RuleFetch`, `RulesStale`)
- [ClearURLs Rules Repo](https://github.com/ClearURLs/Rules)
- [jsDelivr CDN for ClearURLs](https://cdn.jsdelivr.net/gh/ClearURLs/Rules@master/data.min.json)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)