# 0006 - Security Architecture: SSRF Protection, Rate Limiting, Input Validation, Secrets

**Status**: Accepted

**Date**: 2026-11-04

## Context

URLCleanseBot processes untrusted URLs from Telegram users. Attack surface includes:
- **SSRF**: URL expansion follows redirects (t.co, bit.ly) — must not reach internal services
- **Injection**: User-supplied URLs in messages, inline queries, custom rules (regex)
- **DoS**: Large URLs, recursive redirects, expensive regex, AI API abuse
- **Secrets exposure**: Bot token, AI API key, webhook secret in memory/logs
- **Webhook spoofing**: Unauthorized POSTs to webhook endpoint

## Decision

**Defense-in-depth security architecture with layered controls at network, application, and data layers.**

### 1. SSRF Protection (`src/shared/security.rs`)

**DNS Pinning with Allowlist**:
- Resolve hostname → IPs via `ToSocketAddrs`
- Block private/reserved ranges (RFC 1918, RFC 6598, loopback, link-local, multicast)
- Allowlist: Only public IPv4/IPv6
- Cache DNS results (TTL 5 min) to prevent TOCTOU

**Redirect Safety**:
- Max 5 redirects
- Each hop re-validated against SSRF rules
- No credentials forwarded (Authorization headers stripped)

**Network Policy** (deployment-level):
- Container `security_opt: no-new-privileges:true`
- Egress allowlist via host firewall (optional): Telegram API, rule sources, AI API

### 2. Rate Limiting

**Two tiers** (both in `src/shared/security.rs`):

| Tier | Scope | Limit | Storage |
|------|-------|-------|---------|
| **Async (per-user)** | Telegram user ID | 10 req/min | `moka::future::Cache` (in-memory) |
| **Sync (global)** | IP / container | 100 req/min | `std::sync::Mutex<HashMap>` |

- **Async**: Applied in bot handlers before sanitization
- **Sync**: Applied in webhook handler (Axum middleware)
- **Burst allowance**: Token bucket (not implemented; fixed window for simplicity)
- **Response**: `429 Too Many Requests` with `Retry-After` header

### 3. Input Validation

**URL Validation** (`src/sanitizer/validation.rs`):
- Max length: 2048 chars (`MAX_URL_LENGTH`)
- Scheme allowlist: `http`, `https` only
- Parse with `url::Url` — rejects malformed
- IDN handling: Punycode decode, validate labels
- Cache validation results (TTL 1 hour, max 10k entries)

**Custom Rule Validation** (user-supplied regex):
- Regex compilation timeout: 100ms (via `regex::RegexBuilder`)
- Max pattern length: 500 chars
- Max rules per user: 50
- Reject patterns with catastrophic backtracking potential (heuristic: nested quantifiers)

**Message/Inline Input**:
- Max message length: 4096 chars (Telegram limit)
- URL extraction via regex — bounded matches (max 10 URLs/message)

### 4. Secrets Management

| Secret | Storage | Rotation | Access |
|--------|---------|----------|--------|
| `BOT_TOKEN` | `.env` file (chmod 600) | Manual | Runtime env only |
| `AI_API_KEY` | `.env` file | Manual | Runtime env only |
| `WEBHOOK_SECRET` | `.env` file | Manual | Runtime env + HMAC verify |
| `DATABASE_URL` | `.env` file | Manual | Runtime env only |

**Runtime Protections**:
- Distroless image: No shell, no `env` command
- `tracing` filters: `url_cleanse_bot=info` — debug logs exclude secrets
- No secrets in structured logs (validated in `logging.rs`)
- HMAC webhook verification: `X-Telegram-Bot-Api-Secret-Token` + optional `X-Hub-Signature-256`

### 5. Webhook Security

- **Secret token**: `WEBHOOK_SECRET` set via `setWebhook`; validated on each request
- **HMAC signature**: Optional `WEBHOOK_HMAC_SECRET` → `X-Hub-Signature-256` header (SHA256)
- **IP allowlist**: Telegram webhook IPs (optional, via `TELEGRAM_WEBHOOK_IPS` env)
- **TLS**: Terminated at reverse proxy (nginx/Caddy); bot receives HTTP on localhost

### 6. Dependency Security

- **Cargo.lock**: Committed; `cargo audit` in CI
- **Minimal dependencies**: Audit via `cargo tree -d`
- **Supply chain**: `cargo deny` checks (advisories, licenses, sources)
- **SBOM**: `cargo cyclonedx` in CI (future)

## Consequences

### Positive
- **SSRF blocked**: Internal services unreachable via URL expansion
- **Rate limits**: Prevents abuse, protects AI costs, fair usage
- **Input bounds**: DoS via oversized inputs mitigated
- **Secrets contained**: No leakage in logs, images, or traces
- **Webhook integrity**: Only Telegram can trigger bot (with secret)

### Negative
- **In-memory rate limit**: Resets on restart; not distributed (acceptable for single instance)
- **DNS pinning TOCTOU window**: 5 min cache TTL (mitigated by short TTL)
- **Regex DoS risk**: Heuristic only; determined attacker could craft bypass
- **No WAF**: Relies on application-layer controls only

## Alternatives Considered

1. **Network-level SSRF (e.g., `iptables` owner match)**: Rejected — complex, not portable, container network namespace issues
2. **External rate limiter (Redis)**: Rejected — adds dependency; in-memory sufficient for single instance
3. **Full regex sandbox (wasmtime)**: Rejected — overkill; pattern length + compilation timeout sufficient
4. **Secrets in Docker secrets / Vault**: Rejected — adds complexity; `.env` with file perms is standard for self-hosted

## Links

- [Security module](../src/shared/security.rs)
- [URL validation](../src/sanitizer/validation.rs)
- [Webhook handler](../src/presentation/telegram/webhook.rs)
- [SSRF protection details](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)
- [Telegram webhook security](https://core.telegram.org/bots/webhooks#security)