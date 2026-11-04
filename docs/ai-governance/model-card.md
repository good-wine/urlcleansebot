# Model Card: URLCleanseBot AI Engine

> **Status**: Optional component — active only when `AI_API_KEY` is configured  
> **Last Updated**: 2026-11-04  
> **Version**: 1.0.0  
> **Governance Owner**: URLCleanseBot Maintainers  

---

## Model Details

| Field | Value |
|-------|-------|
| **Model Name** | URLCleanseBot AI Engine (External Inference) |
| **Version** | 1.0.0 (tracks provider model version via `AI_MODEL`) |
| **Model Type** | LLM inference via OpenAI-compatible API (chat completion) |
| **Architecture** | Provider-dependent (default: GPT-3.5-Turbo / GPT-4 class) |
| **License** | Proprietary (provider terms); URLCleanseBot code: MIT |
| **Authors / Maintainers** | URLCleanseBot Project Maintainers |
| **Contact** | GitHub Issues / Maintainer email |
| **Deployment** | Runtime inference only — no model weights distributed |
| **Provider** | Configurable via `AI_API_BASE` (default: `https://api.openai.com/v1`) |
| **Model Identifier** | Configurable via `AI_MODEL` (default: `gpt-3.5-turbo`) |

---

## Intended Use

### Primary Use Case
**URL tracking parameter sanitization for edge cases** that rule-based engines (ClearURLs, entropy analysis, multi-source rules) cannot reliably detect.

### Scope
- **Input**: Pre-processed URLs (already cleaned by rule engine + entropy filter)
- **Task**: Identify and remove residual tracking parameters; confirm "no tracking found"
- **Output**: Sanitized URL string or unchanged URL with confirmation
- **Languages**: URL syntax (RFC 3986); prompt in English

### Out of Scope
- Content generation, summarization, or transformation beyond parameter removal
- URL validation (handled upstream)
- Short URL expansion (handled by `redirects` service)
- PII detection/redaction beyond tracking parameters (handled by `redact` module)

### User Control
- **Opt-in only**: Controlled per-user via `ai_enabled` flag (default: `false`)
- **Fallback**: Rule-based pipeline executes regardless; AI is additive
- **Transparency**: Users informed when AI path is taken (Telegram UI shows "AI Deep Scan")

---

## Training Data

> **Not Applicable** — URLCleanseBot does not train, fine-tune, or host any model.

### Provider Training Data (for transparency)
| Provider | Model (Default) | Training Cutoff | Data Sources (per provider disclosure) |
|----------|-----------------|-----------------|----------------------------------------|
| OpenAI | `gpt-3.5-turbo` | Sep 2021 | Public internet text, code, books (per OpenAI) |
| OpenAI | `gpt-4` / `gpt-4o` | Varies | Public + licensed data (per OpenAI) |
| Custom / Self-hosted | Per `AI_MODEL` | Per provider | Per provider documentation |

**Governance Note**: Deployers using non-OpenAI endpoints **must** document the provider's training data, licensing, and data handling in this section.

---

## Evaluation

### Metrics Tracked (Production)
| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| **Tracking Removal Precision** | ≥ 95% | Human eval on sampled AI outputs vs ground truth |
| **Tracking Removal Recall** | ≥ 90% | Synthetic test suite with known tracking params |
| **False Positive Rate** (functional params removed) | ≤ 2% | Regression test suite + user reports |
| **Latency (p95)** | ≤ 5 s | OpenTelemetry spans on `sanitize()` call |
| **Error Rate** | ≤ 1% | HTTP 5xx / timeout / parse errors |
| **Cost per 1k Requests** | Budget-defined | Provider billing API + request counter |

### Test Artifacts
- **Golden Set**: `tests/ai_golden_set.jsonl` — 200 URLs with expected sanitized output
- **Adversarial Set**: `tests/ai_adversarial.jsonl` — edge cases (nested params, obfuscation, lookalike params)
- **Regression Suite**: Runs on every PR via CI (`cargo test ai_`)

### Evaluation Cadence
- **Automated**: On every merge to `main` (GitHub Actions)
- **Manual**: Quarterly human review of 50 random production samples
- **Provider Update**: Re-run full suite when `AI_MODEL` changes

---

## Ethical Considerations

### Privacy — Data Egress to Third Party
| Risk | Description | Mitigation |
|------|-------------|------------|
| **PII in Query Parameters** | URLs may contain emails, IDs, tokens, location data in `?query=` | • Opt-in only (default off)<br>• Pre-redaction by `redact` module (strips known PII patterns)<br>• No conversation history — single-turn stateless call |
| **Vendor Data Retention** | Provider may log prompts/responses per their policy | • DPA / BAA required for production<br>• Prefer zero-retention endpoints (e.g., OpenAI `enterprise` or self-hosted)<br>• Document provider retention in `dpia.md` |
| **Cross-Border Transfer** | Default endpoint `api.openai.com` (US) | • EU deployments: configure `AI_API_BASE` to EU region or self-hosted<br>• SCC / Transfer Impact Assessment if needed |

### Consent & Transparency
- **Explicit Opt-In**: User must toggle `AI Deep Scan` in bot settings
- **In-Context Notice**: Telegram response includes "🤖 AI Deep Scan" badge when used
- **Privacy Policy**: Links to provider privacy policy in `/help` and settings

### Bias & Fairness
- **Low Risk**: Task is syntactic (parameter detection), not semantic judgment on user content
- **Language Bias**: Prompt in English; URLs are ASCII/percent-encoded — minimal language dependence

### Environmental Impact
- **Inference Only**: No training compute
- **Estimated**: ~0.5–2 g CO₂e per request (provider-dependent); tracked via cost metrics

---

## Caveats & Limitations

| Limitation | Impact | Workaround |
|------------|--------|------------|
| **Optional Dependency** | AI path unavailable if `AI_API_KEY` unset | Rule-based pipeline always runs (99%+ coverage) |
| **Provider Availability** | SLA dependent on third party | Circuit breaker: 30s timeout → fallback to rule result |
| **Cost Variability** | Per-token billing; unpredictable at scale | Budget alerts; `AI_MODEL` downgrade (turbo → 3.5); disable via config |
| **Model Drift** | Provider updates model without notice | Pin `AI_MODEL` to dated snapshot (e.g., `gpt-3.5-turbo-0125`); re-eval on change |
| **Prompt Injection** | Malicious URL crafted to hijack prompt | Strict prompt template; URL escaped in JSON; no user-controlled prompt parts |
| **Hallucination** | Model may invent params or corrupt URL | Post-validation: `url-sanitize-core` re-parse output; diff vs input |
| **No Audit Trail of Provider** | Cannot inspect provider internals | Contractual: require model card / system card from provider |

---

## Version History

| Version | Date | Changes | Author |
|---------|------|---------|--------|
| 1.0.0 | 2026-11-04 | Initial model card for AI Governance 2026 | Governance Team |

---

## Appendices

### A. Prompt Template (v1)
```text
You are a URL sanitizer. Remove all tracking parameters from the following URL.
Tracking parameters are things like utm_source, fbclid, gclid, and similar marketing/analytics parameters.
Preserve all functional parameters (e.g., search queries, pagination, filters, authentication tokens).
Return ONLY the cleaned URL. If no tracking parameters are found, return the original URL unchanged.

URL: {url}
```

### B. Request/Response Schema
```json
// Request (OpenAI Chat Completions)
{
  "model": "gpt-3.5-turbo",
  "messages": [{"role": "user", "content": "<prompt>"}],
  "temperature": 0,
  "max_tokens": 256
}

// Response (expected)
{
  "choices": [{"message": {"content": "https://example.com/path?param=value"}}]
}
```

### C. References
- [EU AI Act — Annex III (High-Risk AI Systems)](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32024R1689)
- [NIST AI RMF 1.0](https://airc.nist.gov/AI_RMF_Knowledge_Base)
- [ISO/IEC 42001:2023](https://www.iso.org/standard/81230.html)
- [Model Card Toolkit](https://github.com/tensorflow/model-card-toolkit)