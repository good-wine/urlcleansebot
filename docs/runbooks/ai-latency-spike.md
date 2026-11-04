# Runbook: AI API Latency Spike

**Alert:** `URLCleanseBotAILatencySpike` (from `monitoring/alerts.yml`)
**Severity:** Warning (degraded) → Critical (complete outage)
**SLO Impact:** Latency (p95), Quality (AI sanitization fallback)
**Owner:** Backend On-call

---

## Symptoms

- `urlcleansebot_ai_request_duration_seconds` p95 > 30s (timeout)
- `urlcleansebot_ai_requests_total{status=~"5..|timeout"}` spike
- `urlcleansebot_ai_requests_total{status="429"}` increasing (rate limit)
- `urlcleansebot_ai_requests_total{status="401"}` (auth failure)
- Logs: `AI sanitization failed: ...` / `request timed out` / `HTTP 429` / `HTTP 401`
- Fallback to rule-based sanitization only (quality degradation)
- User-visible: inline queries slower, some URLs not fully cleaned

---

## Diagnosis

### 1. Check AI engine logs
```bash
# Filter AI-related logs
docker logs url_cleanse_bot --tail 500 | grep -i "ai\|openai\|anthropic\|sanitize"

# Follow in real time
docker logs url_cleanse_bot -f | grep -i "ai"
```

### 2. Test AI API connectivity directly
```bash
# From host (if API key in env)
export AI_API_KEY=$(grep AI_API_KEY .env | cut -d= -f2)
export AI_API_BASE=$(grep AI_API_BASE .env | cut -d=2 -f2-)
export AI_MODEL=$(grep AI_MODEL .env | cut -d= -f2)

# Test basic connectivity
curl -s -w "\nHTTP: %{http_code}\nTime: %{time_total}s\n" \
  -H "Authorization: Bearer $AI_API_KEY" \
  -H "Content-Type: application/json" \
  "$AI_API_BASE/models" | head -20

# Test completions endpoint (what bot uses)
curl -s -w "\nHTTP: %{http_code}\nTime: %{time_total}s\n" \
  -H "Authorization: Bearer $AI_API_KEY" \
  -H "Content-Type: application/json" \
  -d "{\"model\":\"$AI_MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"test\"}],\"max_tokens\":5}" \
  "$AI_API_BASE/chat/completions"
```

### 3. Check rate limit headers
```bash
# Inspect rate limit response headers
curl -s -D - -o /dev/null \
  -H "Authorization: Bearer $AI_API_KEY" \
  -H "Content-Type: application/json" \
  -d "{\"model\":\"$AI_MODEL\",\"messages\":[{\"role\":\"user\",\"content\":\"test\"}],\"max_tokens\":5}" \
  "$AI_API_BASE/chat/completions" | grep -i "ratelimit\|retry-after"
```

### 4. Check model deprecation
```bash
# List available models
curl -s -H "Authorization: Bearer $AI_API_KEY" "$AI_API_BASE/models" | jq '.data[].id' | grep -E "gpt|claude"

# Check provider status page
# OpenAI: https://status.openai.com
# Anthropic: https://status.anthropic.com
```

### 5. Metrics analysis (Prometheus)
```bash
# AI request rate and error rate
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_ai_requests_total[5m])"
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_ai_requests_total{status=~'5..|timeout'}[5m])"

# Latency percentiles
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.95, rate(urlcleansebot_ai_request_duration_seconds_bucket[5m]))"
curl -s "http://localhost:9090/api/v1/query?query=histogram_quantile(0.99, rate(urlcleansebot_ai_request_duration_seconds_bucket[5m]))"

# Queue depth (if using semaphore)
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_ai_semaphore_waiters"
```

### 6. Trace slow AI requests (OpenTelemetry)
```bash
# Get trace ID from logs
docker logs url_cleanse_bot --tail 100 | grep -i "trace_id\|span_id" | head -5

# Query traces via otelcli (if configured)
otelcli trace get --trace-id <trace_id> --endpoint http://localhost:4318/v1/traces

# Filter for AI spans
otelcli span list --service urlcleansebot --name "ai.sanitize" --endpoint http://localhost:4318/v1/traces | head -20
```

---

## Mitigation (Immediate Workaround)

### Disable AI sanitization temporarily
```bash
# Option 1: Set empty API key (bot will skip AI)
docker exec url_cleanse_bot sh -c 'echo "AI_API_KEY=" >> /app/.env'
docker restart url_cleanse_bot

# Option 2: Set via docker compose (persistent)
# Edit .env: AI_API_KEY=
docker compose up -d
```

### Switch to fallback model (if configured)
```bash
# Edit .env to use cheaper/faster model
# AI_MODEL=gpt-3.5-turbo  # instead of gpt-4
docker compose up -d
```

### Increase timeout (if latency spike is temporary)
```bash
# In src/sanitizer/ai_engine.rs: AI_TIMEOUT_SECS = 30
# Rebuild and deploy (takes ~2 min with cached deps)
docker compose build --no-cache url_cleanse_bot && docker compose up -d
```

### Circuit breaker: Reduce AI usage
```bash
# In config: reduce inline_max_results to limit AI calls
# Edit .env: INLINE_MAX_RESULTS=3
docker compose up -d
```

---

## Resolution (Root Cause Fix)

| Cause | Fix |
|-------|-----|
| **Timeout (30s)** | Provider overload; implement retry with backoff (already in `http_utils::retry_http_request`), add circuit breaker |
| **HTTP 5xx** | Provider incident; check status page; failover to secondary provider if configured |
| **HTTP 429 Rate limit** | Implement token bucket client-side; add `Retry-After` header respect; request quota increase |
| **HTTP 401 Auth** | Rotate API key in `.env`; verify key has correct permissions; check billing |
| **Model deprecated** | Update `AI_MODEL` in `.env` to supported model; test prompt compatibility |
| **Network/DNS** | Check VPS firewall/egress; try `curl` from container; use explicit IP if DNS fails |

### Code improvements (src/sanitizer/ai_engine.rs)
- Add exponential backoff with jitter (currently uses `retry_http_request`)
- Implement client-side rate limiter (token bucket)
- Add circuit breaker pattern (fail fast after N errors)
- Support multiple providers (OpenAI, Anthropic, local Ollama)
- Cache AI responses for identical URLs (TTL 24h)

---

## Rollback Procedure

```bash
# 1. Disable AI immediately (fastest)
docker exec url_cleanse_bot sed -i 's/^AI_API_KEY=.*/AI_API_KEY=/' /app/.env
docker restart url_cleanse_bot

# 2. Or rollback to previous image (if AI code changed)
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest
docker compose up -d

# 3. Verify rule-based sanitization works
docker logs url_cleanse_bot --tail 30 | grep -i "rule\|clearurls"
curl -f http://localhost:3000/healthz
```

---

## Prevention

1. **Multi-provider support**: Configure fallback provider (e.g., Anthropic if OpenAI fails)
2. **Client-side rate limiting**: Token bucket in `AiEngine` respecting `x-ratelimit-*` headers
3. **Circuit breaker**: Open after 5 consecutive failures, half-open after 30s
4. **Response caching**: Cache AI results per URL (LRU, 10k entries, 24h TTL)
5. **Synthetic monitoring**: Scheduled job hitting AI API every 5m; alert on latency > 10s
6. **Budget alerts**: Track AI spend via provider API; alert at 80% monthly quota
7. **Model version pinning**: Pin `AI_MODEL` to specific version (e.g., `gpt-3.5-turbo-0125`), not alias

---

## Links

- [AI Engine implementation](src/sanitizer/ai_engine.rs)
- [HTTP retry utility](src/http_utils.rs)
- [Config: AI settings](src/config.rs#L29-L31)
- [Prometheus metrics](src/metrics.rs) (search `ai_`)
- [Alerts: AI API](monitoring/alerts.yml) (search `AIAPI`)
- [OpenAI Status](https://status.openai.com)
- [Anthropic Status](https://status.anthropic.com)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)