# Contributing to ClearURLs Bot

Thank you for your interest in contributing! We welcome all contributions that help make this project more robust, feature-rich, and secure.

## Quick Start

### Prerequisites

- **Rust 1.88+** (set as `rust-version` in `Cargo.toml`)
- **Git** for version control
- **Podman** (optional, for containerized development)

### Initial Setup

```bash
git clone https://github.com/good-wine/clearurlsbot.git
cd clearurlsbot

cp .env.example .env
# Edit .env with your development configuration

rustup component add rustfmt clippy
```

## Development Workflows

### Local Development

```bash
cargo build            # debug build
cargo run              # run with auto-reload via cargo-watch
cargo test             # run all tests
cargo fmt --check      # check formatting
cargo clippy --all-targets -- -D warnings  # lint
```

### Container Development

```bash
./podman-deploy.sh build
./podman-deploy.sh run
./podman-deploy.sh logs
./podman-deploy.sh stop
```

### Database Development

```bash
# SQLite (default)
cargo run

# PostgreSQL
export DATABASE_URL=postgresql://user:pass@localhost/clearurls_dev
cargo run
```

## Testing

### Pre-commit Checklist

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --all-targets
```

### Test Structure

The project has **90 tests** across 5 categories:

| Suite | Count | Description |
|-------|-------|-------------|
| Unit tests (`cargo test --lib`) | 63 | Sanitizer, redirects, security, helpers, health |
| Bot commands (`cargo test --test bot_commands_tests`) | 8 | Integration tests with in-memory SQLite |
| Database (`cargo test --test database_tests`) | 10 | User configs, history, whitelist, feature flags |
| Sanitizer (`cargo test --test sanitizer_tests`) | 9 | Real ClearURLs rules fetching and URL cleaning |
| Security (`cargo test --test security_tests`) | 0 | See unit tests in `shared/security.rs` |

### Running Tests

```bash
# All tests
cargo test

# Unit tests only
cargo test --lib

# Integration tests
cargo test --test '*'

# Specific test suite
cargo test sanitizer_tests
cargo test database_tests
cargo test bot_commands_tests

# Verbose output
cargo test -- --show-output --nocapture
```

### Test Infrastructure

- Each integration test gets its own **isolated in-memory SQLite database** using `sqlite:file:testdb{id}?mode=memory&cache=shared`
- The `tests/common/mod.rs` module provides shared fixtures (`setup_test_db()`, `test_config()`, sample URLs)
- Sanitizer tests fetch real ClearURLs rules from the internet — they require network access

### Writing Tests

```rust
// Unit test (inline in source file)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        assert_eq!(my_function(), expected);
    }
}

// Integration test (in tests/ directory)
mod common;
use common::setup_test_db;

#[tokio::test]
async fn test_db_operation() {
    let db = setup_test_db().await;
    // ... test code
}
```

## Code Quality Standards

- **Clippy**: All warnings must be addressed or explicitly allowed in `clippy.toml`
- **Formatting**: `cargo fmt --all` — enforced in CI
- **Error Handling**: Use `Result` types, avoid `unwrap()` in production code
- **Logging**: Use `tracing` with appropriate levels (`info!`, `debug!`, `warn!`, `error!`)

## Project Structure

```
src/
├── presentation/telegram/  # Bot handlers, UI, settings, security scans
├── sanitizer/              # URL cleaning engine
├── redirects/              # Alternative frontend detection
├── db/                     # Database layer
├── shared/                 # Error types, security utils
├── application/            # Clean Architecture skeleton
├── domain/                 # Entities and repository interfaces
├── infrastructure/         # Repository implementations
├── config.rs               # Configuration
├── main.rs                 # Orchestrator (~50 lines)
└── lib.rs                  # Module declarations
```

### Adding New Features

1. **New commands/handlers** — Add to `presentation/telegram/handlers.rs`
2. **UI helpers** — Add to `presentation/telegram/helpers.rs` (with tests)
3. **Sanitization rules** — Add to `sanitizer/rule_engine.rs`
4. **Database operations** — Add to `db/implementation.rs` and update `db/models.rs` if needed
5. **New languages** — Add translations in `i18n.rs` (see [LANGUAGES.md](LANGUAGES.md))
6. **Configuration** — Add to `config.rs` with validation and update `.env.example`

## Pull Request Process

1. **Create branch** — descriptive name: `feat/url-sanitization`, `fix/db-connection`
2. **Commit frequently** — descriptive messages following [conventional commits](https://www.conventionalcommits.org/)
3. **Run quality checks** — `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
4. **Create PR** — fill out description, link issues, describe testing

### PR Requirements

- Clear title using conventional commit format (`feat:`, `fix:`, `docs:`, `refactor:`, etc.)
- Description explaining what, why, and how
- All CI checks pass
- Documentation updated if applicable

## Debugging & Troubleshooting

### Common Issues

```bash
# Clean rebuild
cargo clean && cargo build

# Update dependencies
cargo update

# Reset database
rm bot.db && cargo run

# Rebuild container
podman rmi clear_urls_bot
./podman-deploy.sh build
```

### Logging Configuration

```bash
# Debug logging
RUST_LOG=debug cargo run

# Specific module logging
RUST_LOG=clear_urls_bot::sanitizer=trace,clear_urls_bot::presentation::telegram=debug cargo run
```

## Issue Reporting

### Bug Reports

Include:
- **Environment**: Rust version, OS, database type
- **Steps to Reproduce**: Clear, minimal reproduction steps
- **Expected vs Actual Behavior**
- **Logs**: Relevant output with `RUST_LOG=debug`

### Feature Requests

Provide:
- **Use Case**: Why this feature is needed
- **Proposed Solution**: How you envision it working
- **Alternatives Considered**

---

Thank you for contributing to ClearURLs Bot!
