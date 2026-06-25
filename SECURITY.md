# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest `main` | Yes |

## Security Features

### Input Validation & Sanitization

- **Rate limiting** — Async moka future cache, 1 request/second per user (`shared/security.rs`)
- **Input sanitization** — Control character stripping, 4000-char cap
- **Callback sanitization** — Same rules for callback query data
- **URL validation** — Regex + malicious pattern detection (javascript:, data:, vbscript:, file:, ftp:, mailto:)
- **Telegram text escaping** — HTML entity encoding for `<`, `>`, `&`, `"`, `'`
- **Domain validation** — RFC-compliant domain format checking

### Multi-Layer URL Sanitization

- **Rule-based** — ClearURLs + AdGuard + Brave + Firefox rules via `url-sanitize-core`
- **Entropy analysis** — Shannon entropy detection for unknown tracking parameters (>3.0 bits/char)
- **Normalization** — UTM stripping, parameter sorting, canonicalization via `url-normalize`
- **AI engine** — Optional OpenAI-compatible pass for edge cases

### SSRF Protection

- **DNS resolution check** — Short URL expansion resolves the target hostname before following redirects
- **Private IP blocking** — All private, reserved, loopback, link-local, and broadcast IP ranges blocked for both IPv4 and IPv6 (ULA, link-local)
- **Scope**: Applies to all shortlink expansion (bit.ly, tinyurl, etc.) and any URL that requires DNS resolution

### Permission Controls

- **Admin-only actions** — All administrative operations check `ADMIN_ID`
- **Multi-tenant isolation** — All data scoped by `user_id`

### Data Protection

- **No sensitive data in logs** — Tokens, keys, and personal data are never logged
- **Automatic redaction** — Sensitive URL parameters redacted in debug output
- **Environment variables** — `.env` should have restrictive permissions (`chmod 600`)
- **Multi-tenant isolation** — All data scoped by `user_id`

### CI/CD Security

- **`cargo audit`** — Scans dependencies for known vulnerabilities
- **`cargo deny`** — Enforces license compliance and vulnerability policy
- **CodeQL** — GitHub's code scanning on every push
- **OSV-Scanner** — Google's open-source vulnerability scanner
- **Dependabot** — Automated dependency updates (weekly)

### Supply Chain Security

- **Lockfile committed** — `Cargo.lock` ensures reproducible builds
- **Registry sources** — Git dependencies are denied; only crates.io is allowed
- **`cargo deny` bans** — Prohibited crates (openssl, old time) are explicitly blocked

## Reporting a Vulnerability

**Do not report security vulnerabilities through public GitHub issues.**

### Preferred: GitHub Security Advisory

1. Go to the repository's [Security tab](https://github.com/good-wine/urlcleansebot/security)
2. Click **"Report a vulnerability"**
3. Fill out the advisory form

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fixes (if any)

### Coordinated Disclosure

We follow a **90-day coordinated disclosure timeline**:

1. **Day 0** — Report received, acknowledged within 72 hours
2. **Day 1–30** — Fix developed and tested
3. **Day 31–60** — Fix deployed to production
4. **Day 61–90** — Public disclosure after user notification
5. **Day 90+** — Full write-up published (if applicable)

## Best Practices for Administrators

1. **Use strong secrets** — Generate random values for `WEBHOOK_SECRET` (at least 32 hex chars)
2. **Limit admin access** — Set `ADMIN_ID` to a single trusted user
3. **Monitor logs** — Check bot logs for suspicious activity
4. **Keep updated** — Use the latest version with security patches
5. **Use HTTPS for webhooks** — Telegram requires HTTPS for webhook URLs
6. **Restrict `.env` permissions** — `chmod 600 .env`
