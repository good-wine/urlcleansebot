# Runbook: Container OOM Kill / Restart Loop

**Alert:** `URLCleanseBotOOMKill`, `URLCleanseBotHighMemoryUsage` (from `monitoring/alerts.yml`)  
**Severity:** Critical  
**SLO Impact:** Availability (downtime during restarts)  
**Owner:** Backend On-call / Platform

---

## Symptoms

- Container restarts repeatedly: `docker ps` shows `Restarting` or age < 1m
- `docker logs url_cleanse_bot` shows `OOMKilled` or exit code 137
- `urlcleansebot_process_resident_memory_bytes` approaches 512MB limit
- `urlcleansebot_restart_count` increasing rapidly
- Health check failures: `curl -f http://localhost:3000/healthz` fails
- Telegram: bot offline, updates queue up, flood wait on recovery

---

## Diagnosis

### 1. Confirm OOM kill
```bash
# Check container status
docker ps -a --filter name=url_cleanse_bot --format "table {{.Names}}\t{{.Status}}\t{{.RestartCount}}"

# Inspect exit code (137 = SIGKILL = OOM)
docker inspect url_cleanse_bot | jq '.[0].State | {ExitCode, OOMKilled, RestartCount, FinishedAt}'

# System OOM logs
dmesg -T | grep -i "out of memory\|oom_kill" | tail -20
journalctl -u docker -f | grep -i oom
```

### 2. Memory usage timeline
```bash
# Prometheus: memory over time
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_process_resident_memory_bytes"
curl -s "http://localhost:9090/api/v1/query?query=rate(urlcleansebot_process_resident_memory_bytes[1h])"

# Container stats history (if cAdvisor)
curl -s "http://localhost:9090/api/v1/query?query=container_memory_usage_bytes{name='url_cleanse_bot'}"
```

### 3. Identify memory growth pattern
```bash
# Check for leak: steady increase without plateau
docker logs url_cleanse_bot --tail 100 | grep -i "cache\|provider\|rule\|memory\|alloc"

# Rule engine cache size
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_rule_engine_cache_size"

# URL pipeline cache size
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_pipeline_cache_size"

# Providers loaded (each provider has compiled regexes)
curl -s "http://localhost:9090/api/v1/query?query=urlcleansebot_providers_loaded"
```

### 4. Analyze large message handling
```bash
# Check for large message processing
docker logs url_cleanse_bot --tail 500 | grep -i "large\|big\|huge\|megabyte\|size"

# Telegram max message size: 4096 chars text, 50MB file
# But inline queries can have large result sets
```

### 5. Rust allocator / fragmentation
```bash
# Check allocator (default: mimalloc in Rust 1.88+)
# Look for memory fragmentation signs
docker exec url_cleanse_bot cat /proc/self/status | grep -i "vmpeak\|vmhwm\|vmrss"

# Enable jemalloc profiling (rebuild required)
# RUSTFLAGS="-C target-cpu=native" MALLOC_CONF="prof:true,prof_prefix:/tmp/jeprof" cargo build --release
```

---

## Mitigation (Immediate Workaround)

### 1. Increase memory limit (quickest)
```bash
# Edit compose.yml:
# deploy:
#   resources:
#     limits:
#       memory: 1G      # from 512M
#       cpus: '1.0'     # from 0.5
docker compose up -d
```

### 2. Disable memory-intensive features
```bash
# Disable AI (large HTTP buffers, JSON parsing)
# Edit .env: AI_API_KEY=
docker compose up -d

# Disable aggressive mode (extra regex processing)
docker exec url_cleanse_bot sqlite3 /app/bot.db "UPDATE user_configs SET aggressive_mode = 0;"

# Reduce cache sizes
# In src/sanitizer/rule_engine/mod.rs: max_capacity 10_000 -> 1_000
# In src/sanitizer/pipeline.rs: URL_CACHE_MAX_CAPACITY 10000 -> 1000
docker compose build --no-cache url_cleanse_bot && docker compose up -d
```

### 3. Restart with clean state (clears caches)
```bash
docker stop url_cleanse_bot
docker rm url_cleanse_bot
docker compose up -d
# Note: loses in-memory caches, but DB persists
```

### 4. Limit concurrent processing
```bash
# In src/main.rs: reduce Tokio worker threads
# tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()
# Rebuild and deploy
```

---

## Resolution (Root Cause Fix)

| Cause | Evidence | Fix |
|-------|----------|-----|
| **Rule engine cache growth** | `rule_engine_cache_size` grows unbounded | Add `time_to_live` (already 1h) + `weigher` for entry size; max 10k entries |
| **Pipeline URL cache growth** | `pipeline_cache_size` grows unbounded | Add TTL (currently none); LRU eviction; max 10k entries |
| **Providers memory** | 1000+ providers × compiled regexes | Lazy compilation; drop unused providers; Aho-Corasick single automaton |
| **Large message processing** | Spikes on messages with many URLs | Stream processing; limit URLs per message to 50; drop oversized |
| **AI response buffering** | Large JSON responses held in memory | Stream JSON parse; limit response to 4KB |
| **Database connection pool** | Pool holds prepared statements | Reduce `max_connections` from 5 to 3; disable statement caching |
| **Tokio task leak** | Tasks not completing (blocking on I/O) | Add `timeout` on all async ops; `tokio::task::yield_now()` in loops |
| **Memory fragmentation** | RSS grows but heap not | Use `jemalloc` or `mimalloc` (default); `MALLOC_CONF="arena.max:2"` |

### Code fixes

#### Cache bounds (`src/sanitizer/rule_engine/mod.rs`)
```rust
// Add weigher to account for entry size
cache: Cache::builder()
    .max_capacity(10_000)
    .time_to_live(Duration::from_secs(3600))
    .weigher(|_k, v| v.len())  // bytes
    .build(),
```

#### Pipeline cache (`src/sanitizer/pipeline.rs`)
```rust
// Add TTL to URL cache
static PIPELINE_URL_CACHE: LazyLock<Cache<String, String>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(URL_CACHE_MAX_CAPACITY)
        .time_to_live(Duration::from_secs(86400))  // 24h
        .weigher(|_k, v| v.len())
        .build()
});
```

#### Limit concurrent sanitization
```rust
// Semaphore for concurrent pipeline runs (e.g., 50)
static PIPELINE_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(50));

pub async fn run_sanitization_pipeline(...) {
    let _permit = PIPELINE_SEMAPHORE.acquire().await?;
    // ... rest of pipeline
}
```

#### Stream large responses
```rust
// In ai_engine.rs: stream response instead of buffering
let mut stream = client.post(url).json(&body).send().await?.bytes_stream();
// Process chunks, abort if > 4KB
```

---

## Rollback Procedure

```bash
# 1. Stop restart loop
docker stop url_cleanse_bot

# 2. Increase memory limit immediately
# Edit compose.yml: memory: 1G
docker compose up -d

# 3. If code change caused leak, rollback image
docker pull ghcr.io/your-org/urlcleansebot:<previous-tag>
docker tag ghcr.io/your-org/urlcleansebot:<previous-tag> url_cleanse_bot:latest
docker compose up -d

# 4. Verify stable memory
watch -n 5 'docker stats url_cleanse_bot --no-stream'
# Memory should plateau < 80% limit
```

---

## Prevention

1. **Memory limit headroom**: Set limit 2x expected steady-state (currently 512M, expect ~200M)
2. **Cache bounds**: All caches must have `max_capacity` + `time_to_live` + `weigher`
3. **OOM monitoring**: Alert on `container_memory_usage_bytes / container_spec_memory_limit_bytes > 0.8`
4. **Restart loop detection**: Alert on `rate(container_restart_count[5m]) > 0.1`
5. **Load test memory**: `benches/sanitization.rs` + `wrk -t4 -c100 -d60s` with memory profiling
6. **Profiling in CI**: `cargo build --release && valgrind --tool=massif ./target/release/url_cleanse_bot`
7. **Distroless base**: Already using `gcr.io/distroless/cc-debian12:nonroot` (minimal attack surface)
8. **Health check grace period**: `start_period: 10s` in Dockerfile gives time for cache warmup

---

## Links

- [Dockerfile](Dockerfile) (memory limit, health check, distroless)
- [Compose.yml](compose.yml#L16-L20) (resource limits)
- [Rule Engine cache](src/sanitizer/rule_engine/mod.rs#L29-L32)
- [Pipeline cache](src/sanitizer/pipeline.rs#L13-L17)
- [AI Engine](src/sanitizer/ai_engine.rs) (response buffering)
- [Prometheus metrics](src/metrics.rs) (search `process_resident_memory`, `cache_size`)
- [Alerts: OOM](monitoring/alerts.yml) (search `OOMKilled`, `RestartLoop`)
- [Rust memory profiling](https://github.com/rust-lang/rust/issues/128127)
- [SLO Dashboard](https://grafana.your-org/d/urlcleansebot-slo)