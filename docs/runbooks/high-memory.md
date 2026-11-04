# Runbook: High Memory Usage (>85% of Limit)

**Alert:** `URLCleanseBotHighMemoryUsage` (from `monitoring/alerts.yml`)  
**Severity:** Warning  
**SLO Impact:** Precursor to OOM kill / restart loop  
**Owner:** Backend On-call / Platform

---

## Symptoms

- `urlcleansebot_memory_usage_bytes / 536870912 > 0.85` (85% of 512MB)
- `container_memory_usage_bytes` trending upward over hours/days
- No OOM kill yet, but approaching limit
- GC pressure: increased CPU, latency spikes
- Container may be throttled (CPU) due to memory pressure

---

## Diagnosis

### 1. Current memory usage
```bash
# Container memory
docker stats url_cleanse_bot --no-stream

# Prometheus: current usage
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_memory_usage_bytes"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_memory_usage_bytes / 536870912"

# Trend over 24h
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_memory_usage_bytes[1h])"
```

### 2. Break down memory consumers
```bash
# Rule engine cache
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rule_engine_cache_size"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rule_engine_cache_bytes"

# Pipeline URL cache
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_pipeline_cache_size"
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_pipeline_cache_bytes"

# Providers loaded (each has compiled regexes)
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_providers_loaded"

# Rust heap (if allocator metrics enabled)
curl -s "http://localhost:9090/api/v1/query?query=process_resident_memory_bytes"
```

### 3. Check for memory growth pattern
```bash
# Steady increase = leak; plateau = cache saturation; spikes = large payloads
docker logs url_cleanse_bot --tail 100 | grep -i "cache\|provider\|alloc\|grow"

# Check allocator stats (if jemalloc)
docker exec url_cleanse_bot cat /proc/self/status | grep -i "vmpeak\|vmhwm\|vmrss\|rssanon"
```

### 4. Correlate with traffic
```bash
# Request rate vs memory
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_requests_total[5m])"
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_sanitization_total[5m])"

# Large message handling
docker logs url_cleanse_bot --tail 500 | grep -i "large\|big\|urls.*[0-9][0-9][0-9]"
```

---

## Mitigation (Immediate Workaround)

### 1. Restart container (clears caches, frees fragmented memory)
```bash
docker restart url_cleanse_bot
# Memory drops to baseline (~100-150MB)
# Monitor for regrowth rate
```

### 2. Reduce cache sizes immediately
```bash
# In .env or runtime (requires restart):
# RULE_CACHE_MAX=1000
# URL_CACHE_MAX=1000
# Or edit src and rebuild
docker compose build --no-cache url_cleanse_bot && docker compose up -d
```

### 3. Increase memory limit (buys time)
```bash
# Edit compose.yml:
# deploy:
#   resources:
#     limits:
#       memory: 1G
docker compose up -d
```

### 4. Disable memory-intensive features
```bash
# Disable AI (large HTTP buffers)
# Edit .env: AI_API_KEY=
docker compose up -d

# Disable aggressive mode
docker exec url_cleanse_bot sqlite3 /app/bot.db "UPDATE user_configs SET aggressive_mode = 0;"
```

---

## Resolution (Root Cause Fix)

| Pattern | Cause | Fix |
|---------|-------|-----|
| **Linear growth, no plateau** | Cache without TTL / eviction | Add `time_to_live` + `weigher` to all `moka::Cache` |
| **Plateau at ~400MB** | Cache saturation (10k entries) | Reduce `max_capacity`; add `weigher` for entry size |
| **Spikes on large messages** | Buffering full response in memory | Stream processing; limit URLs/message to 50 |
| **Growth after rule fetch** | Providers not dropped | Lazy compilation; drop old providers on refresh |
| **Fragmentation (RSS > heap)** | Allocator arena fragmentation | `MALLOC_CONF="arena.max:2,dirty_decay_ms:1000"` |

### Code fixes (priority order)

1. **Pipeline cache TTL** (`src/sanitizer/pipeline.rs`):
```rust
static PIPELINE_URL_CACHE: LazyLock<Cache<String, String>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(URL_CACHE_MAX_CAPACITY)
        .time_to_live(Duration::from_secs(86400))  // 24h TTL
        .weigher(|_k, v| v.len())
        .build()
});
```

2. **Rule engine cache weigher** (`src/sanitizer/rule_engine/mod.rs`):
```rust
cache: Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(3600))
    .weigher(|_k, v| v.len())  // bytes per entry
    .build(),
```

3. **Provider memory** (`src/sanitizer/rule_engine/mod.rs`):
```rust
// Drop old providers on refresh
let mut providers = self.providers.write().unwrap();
*providers = new_providers;  // old ones dropped, regexes freed
```

4. **Stream AI responses** (`src/sanitizer/ai_engine.rs`):
```rust
// Limit response to 4KB, stream parse
let mut body = Vec::with_capacity(4096);
while let Some(chunk) = stream.next().await {
    body.extend_from_slice(&chunk?);
    if body.len() > 4096 { break; }
}
```

5. **Allocator tuning** (Dockerfile or entrypoint):
```dockerfile
ENV MALLOC_CONF="arena.max:2,dirty_decay_ms:1000,muzzy_decay_ms:1000"
```

---

## Rollback Procedure

```bash
# 1. Quick restart to relieve pressure
docker restart url_cleanse_bot

# 2. If leak introduced in recent deploy, rollback image
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest
docker compose up -d

# 3. Verify memory stable < 70% limit for 1h
watch -n 60 'curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_memory_usage_bytes/536870912"'
```

---

## Prevention

1. **Cache contracts**: Every `moka::Cache` must have `max_capacity` + `time_to_live` + `weigher`
2. **Memory budget**: Allocate 512M limit; target steady-state < 200M (40%)
3. **Continuous profiling**: `cargo build --release && valgrind --tool=massif ./target/release/url_cleanse_bot`
4. **Load test with memory tracking**: `benches/sanitization.rs` + `wrk -t4 -c100 -d300s` + `docker stats`
5. **Alert on growth rate**: `rate(memory_usage_bytes[1h]) > 10MB/h`
6. **Allocator defaults**: Use `mimalloc` (Rust 1.88+ default) or `jemalloc` with tuned arenas

---

## Links

- [Pipeline cache](src/sanitizer/pipeline.rs#L13-L17)
- [Rule engine cache](src/sanitizer/rule_engine/mod.rs#L29-L32)
- [AI Engine](src/sanitizer/ai_engine.rs)
- [Dockerfile](Dockerfile)
- [Compose memory limit](compose.yml#L19)
- [OOM kill runbook](docs/runbooks/oom-kill.md)
- [Container restart runbook](docs/runbooks/container-restart.md)
- [Alerts: HighMemoryUsage](monitoring/alerts.yml#L333-L340)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)