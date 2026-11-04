# 0002 - Sanitization Strategy: Rule-Based + Entropy + AI Fallback

**Status**: Accepted

**Date**: 2026-11-04

## Context

URLCleanseBot sanitizes URLs by removing tracking parameters. The threat model includes:
- Known tracking parameters (UTM, fbclid, gclid, etc.) — well-documented
- Unknown/obfuscated tracking parameters — new patterns emerge constantly
- Functional parameters that must be preserved (sort, filter, pagination, auth tokens)
- Shortened URLs that need expansion before sanitization
- Affiliate/creator links that users may want to preserve (opt-in)

The sanitization pipeline (`run_sanitization_pipeline` in `src/sanitizer/pipeline.rs`) chains multiple strategies.

## Decision

**Use a layered sanitization pipeline with deterministic rule-based cleaning as primary, entropy analysis as heuristic detector, and optional AI as fallback for edge cases.**

### Pipeline Order (executed sequentially, each can modify the URL)

1. **URL Expansion** (`RuleEngine::expand_url`) — Resolve short URLs (t.co, bit.ly, etc.) via HEAD requests with SSRF protection
2. **Multi-Source Rule Engine** (`MultiSourceSanitizer`) — ClearURLs + AdGuard + Brave + Firefox rules via `url-sanitize-core` crate
3. **Linkumori Community Rules** (`LinkumoriEngine`) — Additional community-sourced rules
4. **Custom User Rules** — User-defined regex patterns (stored in DB)
5. **Entropy Analysis** (`EntropyAnalyzer`) — Detect high-entropy parameters (>3.0 bits/char) suggesting tracking IDs
6. **Aggressive Mode** (`AggressiveSanitizer`) — Remove all unrecognized parameters (opt-in, per-user)
7. **AI Engine** (`AiEngine::sanitize`) — OpenAI-compatible API for complex/ambiguous cases (opt-in, per-user)
8. **Honor Creator** (`honor_creator_mod`) — Preserve affiliate parameters if user enabled (opt-in)
9. **Normalization** (`normalize_url`) — Canonicalize: lowercase host, remove www, sort params, strip known UTM

Each step receives the output of the previous step. Early exit if URL unchanged after step 2-3 (cache hit).

### Priority & Fallback Logic

| Layer | Trigger | Latency | Deterministic | Privacy |
|-------|---------|---------|---------------|---------|
| Rules (1-4) | Always | ~1-5ms | Yes | Local |
| Entropy (5) | Always | ~1ms | Yes | Local |
| Aggressive (6) | User opt-in | ~1ms | Yes | Local |
| AI (7) | User opt-in + rules uncertain | ~500-3000ms | No | External API |
| Honor Creator (8) | User opt-in | ~1ms | Yes | Local |

**AI is strictly fallback**: Only invoked when:
- User has `ai_enabled=true` in config
- API key configured
- Rule-based result has low confidence (heuristic: many high-entropy params remain)
- Not a cache hit

### Entropy Threshold

Shannon entropy > 3.0 bits/char flags a parameter value as probable tracking ID. Also detects UUID, Base64, hex-hash patterns via regex. False positives minimized by:
- Whitelist known functional high-entropy params (session IDs, CSRF tokens)
- Only flag params not in rule databases

## Consequences

### Positive
- **Defense in depth**: Multiple independent detection methods
- **Privacy-first**: 95%+ of sanitization happens locally, no data leaves user's VPS
- **Configurable aggressiveness**: Users choose trade-off via settings
- **Cache-friendly**: Deterministic steps enable URL-level caching (`PIPELINE_URL_CACHE`)
- **Observable**: Each step emits metrics (`sanitizer_step_duration_ms`, `sanitizer_step_result`)

### Negative
- **Complexity**: Multiple moving parts, ordering matters
- **AI cost/latency**: Optional but adds external dependency
- **Entropy false positives**: May strip functional params (mitigated by whitelist)
- **Rule freshness**: Depends on upstream rule updates (ClearURLs, etc.)

## Alternatives Considered

1. **AI-only**: Rejected — privacy, cost, latency, non-deterministic
2. **Rules only**: Rejected — misses novel tracking patterns
3. **Entropy only**: Rejected — high false positive rate without rule context
4. **Parallel execution with voting**: Rejected — adds complexity; sequential with early exit is simpler and faster for common case

## Links

- [Pipeline implementation](../src/sanitizer/pipeline.rs)
- [Rule engine](../src/sanitizer/rule_engine/)
- [Entropy analyzer](../src/sanitizer/entropy.rs)
- [AI engine](../src/sanitizer/ai_engine.rs)
- [Multi-source sanitizer](../src/sanitizer/multi_source.rs)
- [ClearURLs rules](https://github.com/ClearURLs/Rules)
- [AdGuard tracking rules](https://github.com/AdguardTeam/TrackingFilters)