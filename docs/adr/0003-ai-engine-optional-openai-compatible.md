# 0003 - AI Engine: Optional OpenAI-Compatible Fallback

**Status**: Accepted

**Date**: 2026-11-04

## Context

The sanitization pipeline (ADR-0002) uses rule-based and entropy-based methods as primary. However, novel tracking patterns, obfuscated parameters, and site-specific tracking can evade deterministic rules.

An optional AI engine provides a fallback for edge cases where rules are uncertain.

### Requirements
- **Opt-in only**: Disabled by default; user must explicitly enable
- **Privacy-aware**: URL sent to external API only when user consents
- **Cost-controlled**: Predictable per-request cost; no runaway spending
- **Provider-agnostic**: OpenAI-compatible API (works with OpenAI, Ollama, vLLM, LocalAI, etc.)
- **Evaluatable**: Quality measurable via eval harness
- **Fail-open**: If AI fails, fall back to rule-based result

## Decision

**Implement AI sanitization as an optional, user-opt-in fallback using an OpenAI-compatible API client with strict timeouts, cost limits, and privacy controls.**

### Configuration (via environment variables)

| Variable | Default | Description |
|----------|---------|-------------|
| `AI_API_KEY` | (none) | API key; if unset, AI engine is disabled globally |
| `AI_API_BASE` | `https://api.openai.com/v1` | Base URL for OpenAI-compatible endpoint |
| `AI_MODEL` | `gpt-3.5-turbo` | Model identifier |
| `AI_TIMEOUT_SECS` | `30` | Request timeout (hardcoded constant) |

Per-user control via `UserConfig.ai_enabled` (DB) — default `false`.

### Implementation

- **Client**: `reqwest::Client` with 30s timeout, retry logic via `http_utils::retry_http_request`
- **Prompt**: Structured system prompt instructing to remove tracking params only, preserve functional params, return only cleaned URL
- **Response parsing**: Extract cleaned URL from JSON response; validate with `url::Url::parse`
- **Fallback**: On any error (timeout, HTTP error, parse error, validation error), return `Ok(None)` → pipeline continues with rule-based result

### Privacy Controls

1. **No logging of URLs to AI**: `tracing::debug` logs only "AI sanitization requested for URL" (no URL value)
2. **Opt-in at two levels**: Global (API key set) + Per-user (`ai_enabled=true`)
3. **Data minimization**: Only the expanded URL sent; no user IDs, chat context, or history
4. **Local alternative**: Users can point `AI_API_BASE` to local Ollama/LocalAI for zero egress

### Cost Controls

- **Model choice**: Default `gpt-3.5-turbo` (~$0.0015/1K tokens); users can choose cheaper models
- **Token limit**: Prompt ~200 tokens, response ~100 tokens → ~$0.00045 per request
- **Rate limiting**: Inherits bot-level rate limit (10 req/min per user)
- **No batching**: Single URL per request (simplicity > batching efficiency at this scale)

### Evaluation Strategy

- **Golden set**: Curated test cases (URL → expected cleaned URL) in `tests/ai_eval/`
- **Metrics**: Precision/recall on tracking param removal; false positive rate on functional params
- **Regression gate**: CI runs eval on PR; blocks merge if F1 drops >5%
- **LLM-as-judge**: For ambiguous cases, use stronger model to grade output

## Consequences

### Positive
- **Handles edge cases**: Novel/obfuscated tracking parameters
- **User control**: Opt-in at two levels; local deployment option
- **Provider flexibility**: Works with any OpenAI-compatible endpoint
- **Fail-safe**: Errors degrade gracefully to rule-based result
- **Observable**: Metrics track AI requests, latency, errors, fallback rate

### Negative
- **External dependency**: Requires network egress; fails if API unavailable
- **Cost**: Per-request cost (small but non-zero)
- **Latency**: 500-3000ms added for AI requests
- **Non-deterministic**: Same input may yield different output across calls/models
- **Privacy risk**: URL leaves user's VPS (mitigated by opt-in + local option)

## Alternatives Considered

1. **Built-in ML model (ONNX/Rust)**: Rejected — model size, maintenance, inference latency on CPU
2. **Always-on AI (no opt-in)**: Rejected — privacy violation, cost, latency
3. **Self-hosted only (no cloud API)**: Rejected — excludes users without GPU/resources
4. **Rule-only with more frequent updates**: Rejected — cannot catch semantic/obfuscated tracking

## Links

- [AI Engine implementation](../src/sanitizer/ai_engine.rs)
- [Pipeline integration](../src/sanitizer/pipeline.rs#L80-L120)
- [Configuration](../src/config.rs#L29-L31)
- [OpenAI API compatibility](https://platform.openai.com/docs/api-reference)
- [Ollama OpenAI compatibility](https://github.com/ollama/ollama/blob/main/docs/openai.md)