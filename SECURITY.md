# 🔒 Security Policy

## 🔐 Security Overview

ClearURLs Bot takes security seriously. This document outlines our security measures, supported versions, and how to report security vulnerabilities.

## 📋 Supported Versions

We actively maintain security updates for the following versions:

| Version | Supported          | Security Updates |
| ------- | ------------------ | ---------------- |
| 1.4.x   | :white_check_mark: | Full support     |
| 1.3.x   | :warning:          | Critical only    |
| < 1.3   | :x:                | Not supported    |

## 🛡️ Security Features

### Built-in Security Measures
- **Rate Limiting**: Prevents abuse with configurable request limits
- **Input Validation**: All user inputs are sanitized and validated
- **Permission Controls**: Strict admin permission checks
- **Data Protection**: Sensitive data encrypted in logs and environment variables
- **Container Security**: Rootless Podman execution recommended
- **HTTPS Webhooks**: Secure webhook communication with secret validation

### External Security Integrations
- **VirusTotal API**: Real-time malware detection with 70+ antivirus engines
- **URLScan.io**: Behavioral analysis and web reputation scoring
- **Automatic URL Sanitization**: Removal of tracking parameters and malicious links

## 🚨 Reporting a Vulnerability

If you discover a security vulnerability, please follow these steps:

### 1. Do Not Create Public Issues
**Do not report security vulnerabilities through public GitHub issues.**

### 2. Contact Us Privately
Send security reports to: [security@clearurlsbot.com](mailto:security@clearurlsbot.com)

Include the following information:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fixes (if any)

### 3. Response Timeline
- **Initial Response**: Within 24 hours
- **Vulnerability Assessment**: Within 72 hours
- **Fix Development**: Within 1-2 weeks for critical issues
- **Public Disclosure**: After fix deployment and user migration

### 4. Responsible Disclosure
We follow responsible disclosure practices:
- We will acknowledge receipt of your report
- We will provide regular updates on our progress
- We will credit you (if desired) once the issue is resolved
- We ask that you allow us reasonable time to fix the issue before public disclosure

## 🔧 Security Best Practices for Users

### For Bot Administrators
1. **Use Strong Secrets**: Generate random, long values for `COOKIE_KEY` and webhook secrets
2. **Limit Admin Access**: Set `ADMIN_ID` to a single trusted user
3. **Enable Security Features**: Configure VirusTotal and URLScan.io API keys
4. **Monitor Logs**: Regularly check bot logs for suspicious activity
5. **Keep Updated**: Use supported versions with latest security patches

### For End Users
1. **Verify Bot Source**: Only use official bot instances
2. **Be Cautious with Links**: The bot helps clean URLs, but always verify suspicious links manually
3. **Report Issues**: Help improve security by reporting unusual bot behavior

## 🔍 Security Audit

This project undergoes periodic security audits. The last audit was completed on **March 2026**.

### Audit Scope
- Code review for security vulnerabilities
- Dependency analysis for known vulnerabilities
- Container security assessment
- API security validation

### Audit Results
- ✅ No critical vulnerabilities found
- ✅ Dependencies are up-to-date and secure
- ✅ Container images follow security best practices
- ✅ API integrations are properly secured

## 📞 Contact

For security-related questions or concerns:
- **Email**: [security@clearurlsbot.com](mailto:security@clearurlsbot.com)
- **Response Time**: Within 24 hours for security issues
