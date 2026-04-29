# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest `main` | Yes |
| < 1.3 | No |

## Security Features

### Input Validation & Sanitization

- **Rate limiting** — moka sync cache with 1 req/sec per user
- **Input sanitization** — control character stripping, 4000-char cap (`shared/security.rs`)
- **Callback sanitization** — same rules for callback query data
- **URL validation** — regex + malicious pattern detection (javascript:, data:, vbscript:, file:, ftp:, mailto:)
- **Telegram text escaping** — HTML entity encoding for `<`, `>`, `&`, `"`, `'`
- **Domain validation** — RFC-compliant domain format checking

### Permission Controls

- **Admin-only actions** — all administrative operations check `ADMIN_ID`
- **Multi-tenant isolation** — all data scoped by `user_id`

### External Security Integrations (Optional)

- **VirusTotal API** — Real-time malware detection with 70+ antivirus engines
- **URLScan.io** — Behavioral analysis and web reputation scoring
- Both are disabled by default and require explicit API key configuration

### Data Protection

- **No sensitive data in logs** — tokens, keys, and personal data are never logged
- **Automatic redaction** — sensitive URL parameters redacted in debug output
- **Environment variables** — `.env` file should have restrictive permissions (`chmod 600`)
- **Permission validation** — callback query data verifies user ownership before processing
- **Language isolation** — user language preference scoped per user, no cross-user leakage

### Container Security

- Rootless Podman execution
- Non-root user in container
- SELinux file labeling

## Reporting a Vulnerability

**Do not report security vulnerabilities through public GitHub issues.**

Send security reports to: [security@clearurlsbot.com](mailto:security@clearurlsbot.com)

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fixes (if any)

### Response Timeline

- **Initial Response**: Within 24 hours
- **Vulnerability Assessment**: Within 72 hours
- **Fix Development**: Within 1-2 weeks for critical issues
- **Public Disclosure**: After fix deployment

## Best Practices for Administrators

1. **Use strong secrets** — Generate random values for `WEBHOOK_SECRET` (16-256 chars, alphanumeric + `_` + `-`)
2. **Limit admin access** — Set `ADMIN_ID` to a single trusted user
3. **Enable security scanning** — Configure VirusTotal and URLScan.io API keys
4. **Monitor logs** — Check bot logs for suspicious activity (`RUST_LOG=debug`)
5. **Keep updated** — Use the latest version with security patches
6. **Use HTTPS for webhooks** — Telegram requires HTTPS for webhook URLs
7. **Restrict `.env` permissions** — `chmod 600 .env`
