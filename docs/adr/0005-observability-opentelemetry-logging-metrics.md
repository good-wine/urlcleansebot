# 0005 - Observability: OpenTelemetry + Structured Logging + Metrics

**Status**: Accepted

**Date**: 2026-11-04

## Context

A self-hosted bot needs observability to answer:
- Is it healthy? (uptime, error rate)
- How is it performing? (latency, throughput)
- What failed? (error context, traces)
- How is the sanitization pipeline behaving? (step timing, cache hits, AI fallback rate)

Constraints:
- Single instance, no observability backend mandated
- Must work with standard tools (Prometheus, Grafana, Jaeger, Loki)
- Low overhead (<5% CPU, <50MB RAM)
- Structured logs for log aggregation

## Decision

**Implement three-pillar observability: OpenTelemetry traces, Prometheus metrics, structured JSON logs — all optional and configurable via environment.**

### Pillars

| Pillar | Implementation | Export |
|--------|----------------|--------|
| **Traces** | `opentelemetry` + `opentelemetry_otlp` + `tracing_opentelemetry` | OTLP/HTTP to collector (Jaeger, Tempo, etc.) |
| **Metrics** | `prometheus` crate (counters, gauges, histograms) | `/metrics` endpoint (Prometheus scrape) |
| **Logs** | `tracing` + `tracing-subscriber` (JSON in production) | stdout → Docker json-file → Loki/Vector |

### Configuration

| Env Var | Default | Description |
|---------|---------|-------------|
| `APP_ENV` | `development` | `production` → JSON logs |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | (unset) | If set, enables OTLP trace export |
| `OTEL_SERVICE_NAME` | `urlcleansebot` | Service name in traces |
| `METRICS_PORT` | `3000` | Same as webhook port; `/metrics` path |

### Trace Instrumentation

- **HTTP client**: `reqwest` middleware adds trace context to outbound requests (Telegram API, rule fetches, AI API)
- **Sanitization pipeline**: Each step is a span with attributes (`step`, `duration_ms`, `result`, `url_hash`)
- **Database**: `sqlx` queries traced via `tracing` instrumentation
- **Bot handlers**: Each update (message, inline, callback) is a root span

### Metrics (defined in `src/metrics.rs`)

**Counters**: `requests_*_total`, `sanitizations_*_total`, `ai_requests_total`, `ai_fallback_total`, `errors_total`

**Gauges**: `db_pool_connections`, `cache_size`, `active_users`

**Histograms**: `sanitization_duration_ms`, `ai_latency_ms`, `db_query_duration_ms`, `http_request_duration_ms`

### Log Structure (Production JSON)

```json
{
  "timestamp": "2026-11-04T12:34:56.789Z",
  "level": "INFO",
  "target": "url_cleanse_bot::sanitizer::pipeline",
  "fields": {
    "step": "rule_engine",
    "duration_ms": 3,
    "removed_params": 5,
    "url_hash": "a1b2c3d4"
  },
  "span": { "trace_id": "...", "span_id": "..." }
}
```

No PII in logs: URLs hashed (first 8 chars of SHA256), user IDs not logged (only in trace attributes if needed).

### SLO/SLI Alignment

| SLI | Target | Measurement |
|-----|--------|-------------|
| **Availability** | 99.5% | `up` metric + health check success |
| **Latency (p95)** | <500ms | `sanitization_duration_ms` histogram |
| **Error rate** | <0.1% | `errors_total / requests_total` |
| **AI fallback rate** | <5% | `ai_fallback_total / sanitizations_total` |

Alert rules (PrometheusRule) suggested in `docs/observability/alerts.yml`.

## Consequences

### Positive
- **Vendor-neutral**: OTLP works with any backend
- **Progressive enhancement**: Works with zero config (logs only); add OTLP endpoint for traces
- **Low overhead**: Prometheus metrics are in-memory; OTLP batch export async
- **Correlation**: Trace IDs in logs enable log-trace correlation
- **Pipeline visibility**: Per-step timing reveals bottlenecks

### Negative
- **Dependency weight**: `opentelemetry` + `opentelemetry_otlp` + `tracing_opentelemetry` add compile time and binary size
- **No built-in UI**: Requires external stack (Grafana + Tempo + Loki + Prometheus)
- **Trace sampling**: Not implemented (low volume makes it unnecessary)
- **Cardinality risk**: `url_hash` label on histograms — bounded by cache size

## Alternatives Considered

1. **Logging only**: Rejected — no latency distributions, no distributed trace context
2. **Datadog/NewRelic agent**: Rejected — vendor lock-in, cost, overkill
3. **Custom metrics endpoint only**: Rejected — Prometheus format is standard, interoperable
4. **Full OpenTelemetry SDK with auto-instrumentation**: Rejected — Rust auto-instrumentation immature; manual is fine for this scope

## Links

- [Logging init](../src/logging.rs)
- [Metrics definitions](../src/metrics.rs)
- [Library imports](../src/lib.rs#L9-L12)
- [OpenTelemetry Rust](https://opentelemetry.io/docs/instrumentation/rust/)
- [Prometheus Rust](https://docs.rs/prometheus/latest/prometheus/)