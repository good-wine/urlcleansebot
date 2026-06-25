# Contributing to URLCleanseBot

Thank you for your interest in contributing!

## Quick Start

### Prerequisites

- **Rust 1.88+** (edition 2024)
- **Git** for version control
- **just** (optional, recommended) — `cargo install just`

### Setup

```bash
git clone https://github.com/good-wine/urlcleansebot.git
cd urlcleansebot

cp .env.example .env
# Edit .env with your development configuration

# Option A: with just (recommended)
just setup

# Option B: manual
rustup component add rustfmt clippy
git config core.hooksPath .githooks
```

The pre-commit hook (`just pre-commit`) runs `cargo check`, `cargo fmt --check`, and
`cargo clippy` automatically before each commit.

## Development Workflows

### With just

```bash
just check        # fast compilation check
just fix          # auto-fix clippy + format
just test         # run all tests
just ci           # full CI pipeline
just build        # release build
just pre-commit   # run pre-commit checks
```

### Manual

```bash
cargo build                           # debug build
cargo check --locked --all-targets    # compilation check
cargo fmt --all && cargo fmt --check  # format + verify
cargo clippy --all-targets -- -D warnings  # lint
cargo test                            # all tests
```

## Conventional Commits

This project follows [Conventional Commits](https://www.conventionalcommits.org/):

| Prefix | Usage |
|--------|-------|
| `feat:` | New feature |
| `fix:` | Bug fix |
| `refactor:` | Code change that neither fixes a bug nor adds a feature |
| `docs:` | Documentation only |
| `test:` | Adding/improving tests |
| `chore:` | Build, CI, tooling |
| `perf:` | Performance improvement |
| `ci:` | CI configuration |
| `style:` | Formatting, missing semicolons, etc. |

## Testing

```bash
# All tests
cargo test

# Specific suites
cargo test --lib                          # unit tests
cargo test --test bot_commands_tests      # command integration
cargo test --test database_tests          # database
cargo test --test sanitizer_tests         # sanitizer (needs network)

# Property-based tests (proptest)
cargo test normalize_is_idempotent        # single proptest
```

### Test Structure

- **Unit tests** — inline in each source module (`#[cfg(test)]`)
- **Property-based tests** — `proptest` invariants for URL normalization
- **Integration tests** — `tests/` directory, isolated in-memory SQLite databases

## Project Structure

```
src/
├── presentation/telegram/  # Bot handlers, commands, UI
├── sanitizer/              # URL cleaning engine
├── redirects/              # Alternative frontend detection
├── db/                     # Database layer
├── shared/                 # Error types, security utils
├── metrics.rs              # Prometheus counters
├── config.rs               # Configuration
├── main.rs                 # Orchestrator (~50 lines)
└── lib.rs                  # Module declarations
```

### Adding New Features

1. **Commands** — Add to `presentation/telegram/commands.rs`, register in `handlers.rs`
2. **UI helpers** — Add to `presentation/telegram/helpers.rs` (with tests)
3. **Sanitization rules** — `sanitizer/rule_engine.rs` or `sanitizer/multi_source.rs`
4. **Database ops** — `db/implementation.rs`, update `db/models.rs`
5. **Languages** — Add translations in `i18n.rs` + language code to `SUPPORTED_LANGUAGES` in `helpers.rs` (see [LANGUAGES.md](LANGUAGES.md))
6. **Configuration** — `config.rs` + `.env.example`
7. **Metrics** — Atomic counter in `metrics.rs` + `render_prometheus()`

## Code Quality Standards

| Requirement | Check |
|-------------|-------|
| **Clippy** | `cargo clippy --all-targets -- -D warnings` |
| **Formatting** | `cargo fmt --all -- --check` |
| **Compilation** | `cargo check --locked --all-targets` |
| **Tests** | `cargo test` — all pass |
| **Errors** | `AppResult<T>`, no `unwrap()` in production, no `anyhow` |
| **Logging** | `tracing` with levels: `info!`, `debug!`, `warn!`, `error!` |

## Pull Request Process

1. **Branch** — descriptive name: `feat/`, `fix/`, `refactor/`, `docs/`, etc.
2. **Commit** — conventional commit messages
3. **Quality** — `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
4. **PR** — fill the template, link issues, describe testing

### PR Checklist

- [ ] `cargo fmt --all` — no formatting issues
- [ ] `cargo clippy --all-targets -- -D warnings` — no warnings
- [ ] `cargo test` — all tests pass
- [ ] Documentation updated (if applicable)
- [ ] `.env.example` updated (if config changed)

## Debugging

```bash
RUST_LOG=debug cargo run
RUST_LOG=url_cleanse_bot::sanitizer=trace,url_cleanse_bot::presentation::telegram=debug cargo run
```

## Issue Reporting

### Bug Reports

Include: Rust version, OS, database type, steps to reproduce, logs with `RUST_LOG=debug`.

### Feature Requests

Include: use case, proposed solution, alternatives considered.
