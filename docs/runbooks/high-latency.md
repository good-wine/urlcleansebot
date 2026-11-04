# Runbook: High Latency Sanitization Pipeline (p95 > 2s, p99 > 5s)

**Alert:** `URLCleanseBotHighLatency`, `URLCleanseBotHighLatencyP99` (from `monitoring/alerts.yml`)  
**Severity:** Warning (p95 > 2s) → Critical (p99 > 5s)  
**SLO Impact:** Latency (sanitization p95 < 2s, p99 < 5s)  
**Owner:** Backend On-call

---

## Symptoms

- `urlcleansebot_sanitization_duration_seconds` p95 > 2s, p99 > 5s
- `urlcleansebot_pipeline_stage_duration_seconds{stage="..."}` high for specific stage
- User complaints: "bot slow", "inline takes forever"
- Timeout errors in Telegram (webhook 5s limit, inline 15s limit)
- Container CPU near limit (0.5 core per compose.yml)
- Memory pressure (approaching 512MB limit)

---

## Diagnosis

### 1. Identify slow pipeline stage
```bash
# Overall latency
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(urlcleansebot_sanitization_duration_seconds_bucket[5m]))"
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, rate(urlcleansebot_sanitization_duration_seconds_bucket[5m]))"

# Per-stage latency (rule_engine, ai, entropy, expand, cache)
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, sum by (stage) (rate(urlcleansebot_pipeline_stage_duration_seconds_bucket[5m])))"

# Stage breakdown
curl -s "http://localhost:9090/api/v1/query?query=sum by (stage) (rate(urlcleansebot_pipeline_stage_duration_seconds_sum[5m])) / sum by (stage) (rate(urlcleansebot_pipeline_stage_duration_seconds_count[5m]))"
```

### 2. Check resource utilization
```bash
# Container stats
docker stats url_cleanse_bot --no-stream

# CPU / Memory limits from compose
docker inspect url_cleanse_bot | jq '.[0].HostConfig | {Memory: .Memory, NanoCpus: .NanoCpus}'

# Go/Tokio runtime metrics (if enabled)
curl -s http://localhost:3000/metrics | grep -E "process_cpu_seconds_total|process_resident_memory_bytes|tokio_"
```

### 3. Analyze cache hit rates
```bash
# URL cache hit rate
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_cache_hits_total[5m]) / (rate(urlcleansebot_cache_hits_total[5m]) + rate(urlcleansebot_cache_misses_total[5m]))"

# Rule engine cache
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rule_engine_cache_size"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rule_engine_cache_hits_total"
```

### 4. Profile specific components

#### Rule Engine (ClearURLs matching)
```bash
# Providers loaded count
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_providers_loaded"

# Rules per provider (approx)
docker logs url_cleanse_bot --tail 50 | grep -i "compiled.*provider"

# Check if regex compilation is slow (startup)
docker logs url_cleanse_bot --tail 100 | grep -i "refresh\|compile\|download"
```

#### AI Engine
```bash
# AI latency (see ai-api-down runbook)
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(urlcleansebot_ai_request_duration_seconds_bucket[5m]))"

# AI call rate
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_ai_requests_total[5m])"
```

#### Entropy / Aggressive mode
```bash
# Entropy calculation rate
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_entropy_calculations_total[5m])"

# Aggressive mode enabled users
docker exec url_cleanse_bot sqlite3 /app/bot.db "SELECT count(*) FROM user_configs WHERE aggressive_mode = 1;"
```

#### Database
```bash
# DB query latency
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(urlcleansebot_db_query_duration_seconds_bucket[5m]))"

# Pool contention
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_db_pool_connections_pending"
```

### 5. Trace a slow request (OpenTelemetry)
```bash
# Get trace ID from logs
docker logs url_cleanse_bot --tail 100 | grep -i "trace_id\|span_id" | head -5

# Query traces via otelcli (if configured)
otelcli trace get --trace-id <trace_id> --endpoint http://localhost:4318/v1/traces

# Or query Jaeger/Tempo UI
# UI: https://tempo.your-org/trace/<trace_id>
```

---

## Mitigation (Immediate Workaround)

### 1. Disable AI sanitization (biggest latency contributor)
```bash
# Edit .env: AI_API_KEY=
docker compose up -d
# Reduces p99 by ~2-5s typically
```

### 2. Disable aggressive mode globally
```bash
# SQLite
docker exec url_cleanse_bot sqlite3 /app/bot.db "UPDATE user_configs SET aggressive_mode = 0;"

# Or per-user via bot command: /aggressive off
```

### 3. Increase cache TTL / size
```bash
# In src/sanitizer/rule_engine/mod.rs: Cache TTL 3600 -> 86400
# In src/sanitizer/pipeline.rs: URL_CACHE_MAX_CAPACITY 10000 -> 50000
# Rebuild
docker compose build --no-cache url_cleanse_bot && docker compose up -d
```

### 4. Scale CPU / Memory
```bash
# Edit compose.yml:
# deploy:
#   resources:
#     limits:
#       memory: 1G
#       cpus: '1.0'
docker compose up -d
```

### 5. Enable webhook + background processing
```bash
# Move sanitization to background queue (Redis + workers)
# Return "processing..." immediately
# Requires architecture change
```

---

## Resolution (Root Cause Fix)

| Stage | Typical Latency | Optimization |
|-------|-----------------|--------------|
| **URL Expansion** | 50-500ms | Cache redirects; limit hops to 3; async HTTP |
| **Rule Engine (ClearURLs)** | 10-100ms | Pre-compile regex; use Aho-Corasick for multi-pattern; skip non-matching providers early |
| **Entropy Calculation** | 1-5ms | Already fast; skip if URL < 50 chars |
| **AI Sanitization** | 2-30s | **Disable by default**; async with timeout 3s; cache results 24h; fallback to rules |
| **DB Queries** | 1-50ms | Add indexes; batch reads; connection pool tuning |
| **Cache Lookup** | <1ms | Already fast; monitor hit rate > 90% |

### Code improvements

#### Rule Engine (`src/sanitizer/rule_engine/`)
- **Aho-Corasick**: Replace per-provider regex with single automaton for all tracking params
- **Provider indexing**: Hash URL hostname → provider list (O(1) vs O(n))
- **Lazy compilation**: Compile regex on first match, not at load
- **Bloom filter**: Quick negative check before regex

#### Pipeline (`src/sanitizer/pipeline.rs`)
- **Parallel stages**: Run expansion, entropy, rule-engine concurrently where independent
- **Early exit**: If cache hit, skip all stages
- **Stage timeouts**: Per-stage deadline (expansion 500ms, rules 200ms, AI 3s)

#### AI Engine (`src/sanitizer/ai_engine.rs`)
- **Strict timeout**: 3s max (not 30s); return None on timeout
- **Response caching**: LRU 10k entries, 24h TTL keyed by normalized URL
- **Batch requests**: Combine multiple URLs in single prompt (if inline)

#### Database (`src/db/implementation.rs`)
- **Indexes**: Add on `user_configs(user_id)`, `cleaned_links(user_id, created_at)`
- **Batch inserts**: Accumulate cleaned links, flush every 100 or 1s
- **Read replicas**: If PG, route reads to replica

---

## Rollback Procedure

```bash
# 1. Revert to previous image (known good latency)
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest
docker compose up -d

# 2. Verify latency
docker logs url_cleanse_bot -f | grep -i "sanitization.*duration"
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(urlcleansebot_sanitization_duration_seconds_bucket[5m]))"
```

---

## Prevention

1. **Latency budgets per stage**: Alert if any stage > budget (expansion 500ms, rules 200ms, AI 3s, DB 50ms)
2. **Continuous profiling**: `cargo flamegraph` in CI; track p99 over time
3. **Load testing**: `benches/sanitization.rs` + `wrk`/`hey` against `/sanitize` endpoint
4. **Cache warming**: Pre-populate URL cache with top 1000 domains at startup
5. **Feature flags**: `AI_ENABLED`, `AGGRESSIVE_ENABLED`, `ENTROPY_ENABLED` via config
6. **Canary deploy**: Route 5% traffic to new version; compare latency percentiles
7. **SLO dashboard**: `sanitization_latency_p95`, `sanitization_latency_p99` with burn-rate alerts

---

## Links

- [Pipeline implementation](src/sanitizer/pipeline.rs)
- [Rule Engine](src/sanitizer/rule_engine/mod.rs)
- [AI Engine](src/sanitizer/ai_engine.rs)
- [Entropy](src/sanitizer/entropy.rs)
- [Aggressive mode](src/sanitizer/aggressive.rs)
- [Database](src/db/implementation.rs)
- [Prometheus metrics](src/metrics.rs) (search `sanitization`, `pipeline_stage`, `cache`)
- [Alerts: Latency](monitoring/alerts.yml) (search `HighLatency`, `PipelineLatency`)
- [Benchmarks](benches/sanitization.rs)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)