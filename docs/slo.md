# URLCleanseBot — Service Level Objectives

> **Scope**: Single-instance self-hosted Telegram bot on VPS (Docker Compose).  
> **Observability**: OpenTelemetry (traces, metrics, structured JSON logs) → Prometheus + Grafana.

---

## 1. Service Level Indicators (SLI)

All SLIs are expressed as PromQL queries against the metrics exposed by `urlcleansebot` (see `src/metrics.rs`).

| SLI | Name | Description | PromQL Query |
|-----|------|-------------|--------------|
| **Availability** | `availability_ratio` | % richieste elaborate con successo (non 5xx, non timeout) su totale richieste | `sum(rate(urlcleansebot_requests_total{status!~"5.."}[5m])) / sum(rate(urlcleansebot_requests_total[5m]))` |
| **Latency p50** | `latency_p50` | Latency end-to-end mediana (ricezione msg → reply inviato) | `histogram_quantile(0.50, sum by (le) (rate(urlcleansebot_latency_seconds_bucket[5m])))` |
| **Latency p95** | `latency_p95` | Latency end-to-end p95 | `histogram_quantile(0.95, sum by (le) (rate(urlcleansebot_latency_seconds_bucket[5m])))` |
| **Latency p99** | `latency_p99` | Latency end-to-end p99 | `histogram_quantile(0.99, sum by (le) (rate(urlcleansebot_latency_seconds_bucket[5m])))` |
| **Correctness** | `correctness_ratio` | % URL sanitizzati correttamente (tracking rimosso, no false positive funzionali) | `sum(rate(urlcleansebot_sanitizations_cleaned_total[5m])) / sum(rate(urlcleansebot_sanitizations_total[5m]))` |
| **AI Availability** | `ai_availability_ratio` | % chiamate AI riuscite (non error, non timeout) | `sum(rate(urlcleansebot_ai_requests_total{status!~"error|timeout"}[5m])) / sum(rate(urlcleansebot_ai_requests_total[5m]))` |
| **AI Latency p95** | `ai_latency_p95` | Latency AI p95 (solo quando AI abilitato) | `histogram_quantile(0.95, sum by (le) (rate(urlcleansebot_ai_latency_seconds_bucket[5m])))` |
| **AI Fallback Rate** | `ai_fallback_ratio` | % richieste AI cadute in fallback rule-based | `sum(rate(urlcleansebot_ai_fallback_total[5m])) / sum(rate(urlcleansebot_ai_requests_total[5m]))` |

### Metriche richieste (già presenti o da aggiungere)

| Metrica | Tipo | Note |
|---------|------|------|
| `urlcleansebot_requests_total` | Counter | `type="message\|inline\|callback\|edited"`, `status="success\|error\|timeout"` |
| `urlcleansebot_latency_seconds` | Histogram | End-to-end, buckets: `[0.1, 0.25, 0.5, 1, 2, 5, 10, 30]` |
| `urlcleansebot_sanitizations_total` | Counter | `result="cleaned\|unchanged\|failed"` |
| `urlcleansebot_ai_requests_total` | Counter | `status="success\|error\|timeout"`, `model="..."` |
| `urlcleansebot_ai_latency_seconds` | Histogram | Buckets: `[0.5, 1, 2, 5, 10, 20, 30, 60]` |
| `urlcleansebot_ai_fallback_total` | Counter | Motivo fallback |
| `urlcleansebot_rule_fetch_total` | Counter | `status="success\|error"`, `source="clearurls\|linkumori"` |
| `urlcleansebot_db_pool_usage` | Gauge | `state="active\|idle"` |
| `urlcleansebot_queue_depth` | Gauge | Pending updates (se webhook) |
| `urlcleansebot_container_restarts_total` | Counter | Da cAdvisor / node_exporter |
| `urlcleansebot_memory_usage_bytes` | Gauge | Da cAdvisor |
| `urlcleansebot_disk_usage_bytes` | Gauge | Da node_exporter |
| `urlcleansebot_tls_cert_expiry_seconds` | Gauge | Da cert_exporter |

---

## 2. Service Level Objectives (SLO)

Rolling window: **30 giorni**.

| SLO | Target | Misurazione | Note |
|-----|--------|-------------|------|
| **Availability** | ≥ **99.5%** | `availability_ratio` | 30d rolling. Corrisponde a ~3.6h downtime/mese. |
| **Latency p95** | ≤ **2s** | `latency_p95` | End-to-end incl. Telegram API round-trip. |
| **Latency p99** | ≤ **5s** | `latency_p99` | Picchi accettabili per edge case. |
| **Correctness** | ≥ **99%** | `correctness_ratio` | Tracking removal verified via test suite + sampling. |
| **AI Availability** | ≥ **99%** | `ai_availability_ratio` | Solo quando AI abilitato. |
| **AI Latency p95** | ≤ **10s** | `ai_latency_p95` | Include model inference + network. |
| **AI Fallback Rate** | ≤ **5%** | `ai_fallback_ratio` | Fallback = rule-based. Indica degrado qualità AI. |

### Error Budget (30d window)

| SLO | Budget Totale (30d) | Budget Orario (1h) | Budget 6h | Budget 3d |
|-----|---------------------|-------------------|-----------|-----------|
| Availability 99.5% | 432 min (7.2h) | 8.64 min | 51.8 min | 4.32h |
| Latency p95 ≤ 2s | N/A (latency budget) | — | — | — |
| Correctness 99% | 1% sanitizzazioni | — | — | — |

> **Nota**: Per latency SLO si usa *error budget consumption rate* basato su burn-rate delle richieste che violano la soglia, non tempo assoluto.

---

## 3. Burn-Rate Alert Thresholds

Basati su [Google SRE Workbook — Multi-window, Multi-burn-rate](https://sre.google/workbook/alerting/).

| Burn Rate | Window | Budget Consumed | Severity | Action |
|-----------|--------|-----------------|----------|--------|
| **Fast** | 1h | 2% (≈ 8.6 min) | **Page** (Critical) | Wake on-call, immediate investigation |
| **Medium** | 6h | 5% (≈ 21.6 min) | **Ticket** (Warning) | Create incident ticket, investigate within 4h |
| **Slow** | 3d | 10% (≈ 43 min) | **Investigation** (Info) | Weekly review, capacity planning |

### Calcolo Burn Rate (PromQL)

```promql
# Fast burn (1h window, 2% budget = 14.4x normal error rate)
(
  sum(rate(urlcleansebot_requests_total{status=~"5..|timeout"}[1h]))
  /
  sum(rate(urlcleansebot_requests_total[1h]))
)
/
(
  1 - 0.995
)
> 14.4

# Medium burn (6h window, 5% budget = 6x normal error rate)
(
  sum(rate(urlcleansebot_requests_total{status=~"5..|timeout"}[6h]))
  /
  sum(rate(urlcleansebot_requests_total[6h]))
)
/
(
  1 - 0.995
)
> 6

# Slow burn (3d window, 10% budget = 3x normal error rate)
(
  sum(rate(urlcleansebot_requests_total{status=~"5..|timeout"}[3d]))
  /
  sum(rate(urlcleansebot_requests_total[3d]))
)
/
(
  1 - 0.995
)
> 3
```

---

## 4. Alert Rules — `monitoring/alerts.yml`

Vedi file separato `monitoring/alerts.yml` per regole PrometheusRule complete con:
- Burn-rate alerts (multi-window, multi-burn-rate)
- Symptom-based: error rate, latency, queue depth, DB pool
- AI-specific: error rate, latency spike, cost anomaly
- Infra: container restart, OOM, disk, cert expiry

Ogni alert include:
- `labels.severity`: `critical` | `warning` | `info`
- `labels.runbook`: link a runbook in `docs/runbooks/`
- `annotations.summary` / `description` con query diagnostiche

---

## 5. Runbook References

| Alert | Runbook |
|-------|---------|
| `URLCleanseBotHighErrorRate` | `docs/runbooks/high-error-rate.md` |
| `URLCleanseBotHighLatency` | `docs/runbooks/high-latency.md` |
| `URLCleanseBotQueueDepth` | `docs/runbooks/queue-depth.md` |
| `URLCleanseBotDBPoolExhausted` | `docs/runbooks/db-pool-exhausted.md` |
| `URLCleanseBotAIHighErrorRate` | `docs/runbooks/ai-high-error-rate.md` |
| `URLCleanseBotAILatencySpike` | `docs/runbooks/ai-latency-spike.md` |
| `URLCleanseBotAICostAnomaly` | `docs/runbooks/ai-cost-anomaly.md` |
| `URLCleanseBotContainerRestart` | `docs/runbooks/container-restart.md` |
| `URLCleanseBotOOMKill` | `docs/runbooks/oom-kill.md` |
| `URLCleanseBotDiskFull` | `docs/runbooks/disk-full.md` |
| `URLCleanseBotCertExpiry` | `docs/runbooks/cert-expiry.md` |

---

## 6. Review Cadence

| Attività | Frequenza | Owner |
|----------|-----------|-------|
| SLO compliance review | Settimanale (Mon) | On-call |
| Error budget burn-down | Giornaliero (dashboard) | On-call |
| Alert noise review | Mensile | SRE + Dev |
| SLO target adjustment | Trimestrale | Tech Lead + PM |
| Game Day / Chaos test | Trimestrale | Team |

---

## 7. Escalation Policy

1. **Critical (Page)** → On-call primaria (5 min ack) → Se no ack in 10 min → On-call secondaria
2. **Warning (Ticket)** → Assigned to on-call, investigate within 4h business hours
3. **Info (Investigation)** → Added to weekly triage backlog

---

*Document version: 1.0*  
*Last updated: 2026-11-04*  
*Next review: 2027-02-04*