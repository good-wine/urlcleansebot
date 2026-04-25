# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### 🔧 Technical Improvements

- **HTTP Resilience**: Implemented retry with exponential backoff for all external API calls
  - Added `tokio-retry` dependency for robust retry logic
  - Exponential backoff strategy: starts at 1s, max 30s delay, max 3 attempts
  - Applied to all HTTP requests: VirusTotal API, URLScan.io API, LibRedirect/Farside catalogues, ClearURLs rules download, AI API calls
  - Improved bot reliability when external services experience temporary issues
  - Better error handling and user experience during network problems

- **Configurable External URLs**: Moved hardcoded URLs to configuration
  - Added `LIBREDIRECT_URL` and `FARSIDE_URL` to config with sensible defaults
  - URLs are now configurable via environment variables instead of being hardcoded
  - Maintains backward compatibility with default production URLs
  - Allows for testing with local mock servers or custom endpoints

## [1.4.1] - 2026-03-05

### ✨ Improvements

- **Enhanced Command User Experience**: Improved `/language` and `/setlang` commands with better feedback
  - `/language` now shows current language setting
  - `/setlang` provides clear success/error messages with language names
  - Better formatting with emojis and HTML markup
- **Enhanced Settings User Experience**: Improved settings menus with clearer status display and better user feedback
  - Notifications settings now show current status before allowing changes
  - AI settings display current state and show appropriate toggle action
  - Language change confirmation now uses the new language for response messages
  - Better visual indicators with emojis and clear action buttons
- **Settings System Improvements**: Enhanced callback handling and error management
  - More robust error handling for settings updates
  - Improved user feedback for successful/failed operations
- **Automatic Alternative Frontends**: Removed manual `/redirect` command and implemented automatic detection
  - Bot now automatically detects URLs from popular services (YouTube, Twitter, Reddit, etc.)
  - Automatically suggests privacy-focused alternatives (Invidious, Piped, Nitter, Teddit, etc.)
  - Frontend suggestions appear automatically after URL cleaning
  - Uses existing LibRedirect and Farside data sources with 6-hour cache TTL
  - Better separation of concerns in settings callback processing
- **Command Cleanup**: Removed duplicate `/topusers` command (functionality merged into `/leaderboard`)
  - Streamlined command set for better user experience
  - Updated help text to reflect current command list

### 📚 Documentation Updates

- **README.md**: Added comprehensive "Bot Commands" section with all available commands
- **QUICK_START.md**: Added user guide section explaining how to use bot features
- **i18n.rs**: Added new translation keys for improved settings UI
  - `s_notif_current_status` for displaying current notification status
  - Updated all language translations (IT, EN, ES, FR, DE)
- **Command Documentation**: All bot commands now properly documented with descriptions

### 🔧 Technical Improvements

- **Code Quality**: Verified compilation after command and settings improvements
- **Consistency**: Standardized command response formatting across the bot
- **Error Handling**: Improved error management in settings callbacks

### ⚠️ Breaking Changes

- **Removed**: `/topusers` command (use `/leaderboard` instead)
- **Removed**: `/redirect <url>` command (now automatic - frontend alternatives are suggested automatically when URLs from popular services are detected)

---

## [1.4.0] - 2026-03-04

### 🚀 New Features

- **VirusTotal Integration**: Complete implementation of VirusTotal API v3 for malware detection
  - Automatic URL scanning with 70+ antivirus engines
  - Real-time alerts for malicious and suspicious links
  - Configurable via `VIRUSTOTAL_API_KEY` environment variable
  - Comprehensive documentation in `docs/VIRUSTOTAL.md`
- **Enhanced URL Detection**: Fixed critical bug in URL entity detection
  - URLs are now correctly identified in both private chats and groups
  - Improved message processing pipeline with detailed logging

### 🐛 Bug Fixes

- **Critical**: Fixed `has_urls` flag not being set correctly, causing URLs to be skipped
  - URLs with `MessageEntityKind::Url` or `MessageEntityKind::TextLink` now properly detected
  - Commands no longer incorrectly trigger URL processing
- **Logging**: Added comprehensive debug logging throughout message processing pipeline
  - "Messaggio ricevuto" logs with user_id, chat_id, and text length
  - "Nessun URL trovato" vs "URL candidati trovati" for better debugging
  - "Invio risposta con URL puliti" for tracking successful responses

### 🔧 Technical Improvements

- **Dependencies**: Added `base64 = "0.22"` for VirusTotal URL encoding
- **Security**: VirusTotal requests use proper base64 URL-safe encoding without padding
- **Performance**: 10-second timeout for VirusTotal API calls to prevent blocking
- **Code Quality**: Improved error handling and logging consistency

### 📚 Documentation

- **New**: Complete VirusTotal integration guide (`docs/VIRUSTOTAL.md`)
  - Setup instructions with API key acquisition
  - Rate limits and free tier details (4 req/min, 500/day)
  - Privacy considerations and self-hosted alternatives
  - Troubleshooting and examples
- **Updated**: README.md with VirusTotal feature details
- **Updated**: ARCHITECTURE.md with VirusTotal integration section
- **Updated**: All documentation reflects current codebase state

### ⚠️ Breaking Changes

None - all changes are backward compatible.

### 🔒 Security

- VirusTotal integration is fully optional and disabled by default
- URLs sent to VirusTotal become public in their database (documented)
- No sensitive data logged or transmitted

## [1.3.0] - 2026-02-24

### 🛠 Migliorie principali

- Gestione errori esplicita e logging avanzato
- Modularità estesa: funzioni di sanitizzazione e validazione in moduli dedicati
- Test automatizzati aggiunti per validazione input/output
- Ottimizzazione performance con cache
- Sicurezza input/output rafforzata
- Internazionalizzazione dinamica tramite file JSON
- Documentazione aggiornata

All notable changes to this project will be documented in this file.

## [1.2.0] - 2026-01-20

### 🚀 Major Modernization Release

- **Rust Toolchain Update**: Updated to Rust 1.92+ with MSRV 1.75 for modern language features and performance improvements
- **Podman Migration**: Complete migration from Docker to Podman for enhanced security and rootless container support
- **Build Optimization**: Optimized Cargo.toml with LTO, single codegen unit, and improved release profile settings
- **Performance Improvements**: Enhanced build times and runtime performance through compiler optimizations

### 🔄 Breaking Changes

- **Container Runtime**: Switched from Docker to Podman (Docker still supported but deprecated)
- **Containerfile**: Replaced Dockerfile with Podman-compatible Containerfile
- **Deployment Scripts**: New `podman-deploy.sh` script replacing Docker-based deployment

### 🛠️ Code Quality & Modernization

- **Deprecated API Fixes**: Fixed all deprecated teloxide method calls (`msg.from()` → `msg.from`)
- **Modern Async Patterns**: Updated to use `ReplyParameters` for modern teloxide API
- **Memory Safety**: Replaced `LazyLock` with `once_cell::Lazy` for MSRV compatibility
- **Dependency Updates**: Updated to latest stable versions across all dependencies

### 📦 Container & Deployment Enhancements

- **Security Improvements**: Rootless Podman operation with proper SELinux labeling
- **Pod Networking**: Implemented Podman pod architecture for better network isolation
- **Resource Management**: Enhanced resource limits and monitoring capabilities
- **Multi-stage Builds**: Optimized container image size and security

### 📚 Documentation Overhaul

- **Comprehensive README**: Complete rewrite with modern deployment options and examples
- **Architecture Guide**: Updated with detailed Podman and performance optimization sections
- **Contributing Guide**: Expanded with development workflows, testing guidelines, and code standards
- **Deployment Guide**: New comprehensive deployment documentation covering all scenarios

### 🧪 Testing & Quality Assurance

- **Clippy Compliance**: All clippy warnings resolved with proper code quality standards
- **Build Verification**: Automated build testing for multiple Rust versions
- **Code Formatting**: Consistent rustfmt configuration across the project

## [1.1.0] - 2026-01-10

### Added

- **Smart Language Detection**: Automatically detects language of incoming messages (English/Italian) and replies in the corresponding language.
- **Supabase Integration**: Compatibility with Supabase PostgreSQL for persistent cloud storage.
- **WASM URL Cleaner**: High-performance Rust-compiled WebAssembly module for client-side sanitization.
- **Advanced Observability**: Robust logging system using `tracing` with JSON output support for production and colored pretty-logs for development.
- **Multi-Database Support**: Implemented dynamic backend detection (SQLite/Postgres) using `sqlx::Any`.

### Changed

- Refactored project structure into a modular library (`src/lib.rs`) and binary (`src/main.rs`).
- Upgraded all dependencies to latest major versions (`teloxide 0.17`, `axum 0.8`, `sqlx 0.8`).
- Improved documentation with detailed architecture and observability guides.
- Hardened web dashboard with Axum 0.8 compatibility and enhanced route safety.

### Fixed

- Deprecated `teloxide` method calls and updated to new `reply_parameters` API.
- Fixed `reqwest` TLS feature naming conflicts in version 0.13.
- **Zero-Panic Core**: Eliminated all `unwrap()` calls in favor of graceful error handling and descriptive status codes.
- **Bot Command Handling**: Fixed `/start` command compatibility in group chats and with bot handles.

### Removed

- **WASM Module**: Removed WebAssembly functionality to focus on core bot features and reduce complexity (reverted in 1.2.0 modernization)

---

## Migration Guide

### From 1.1.x to 1.2.0

#### Container Migration

```bash
# Old Docker way (deprecated)
docker-compose up

# New Podman way
./podman-deploy.sh start
# or
podman-compose -f podman-compose.yml up
```

#### Development Setup

```bash
# Update Rust toolchain
rustup update stable

# Rebuild with new optimizations
cargo clean
cargo build --release
```

#### Configuration Changes

- No configuration changes required
- Environment variables remain the same
- Database schema unchanged

### Breaking Changes Summary

- Docker support deprecated (still works but will be removed in future versions)
- Minimum Rust version increased to 1.75 (from 1.70)
- Container image names updated (clear_urls_bot vs previous naming)
- Some internal APIs changed for better performance

---

## Support

For help with migration or issues:

- [GitHub Issues](https://github.com/yourusername/clear_urls_bot/issues)
- [Discussions](https://github.com/yourusername/clear_urls_bot/discussions)
- [Documentation](docs/)
