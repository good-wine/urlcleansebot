# ──────────────────────────────────────────────
# URLCleanseBot — modern command runner (just)
# Install: cargo install just
# Usage:   just <recipe>
# ──────────────────────────────────────────────

default: help

# ── Help ──────────────────────────────────────

# Show available recipes
help:
    @just --list

# ── CI (full pipeline) ────────────────────────

# Run full CI pipeline: check + format + lint + test + security + features
ci: check format-check clippy test deny audit features

# ── Compilation ───────────────────────────────

# Check compilation (fast)
check:
    cargo check --locked --all-targets

# Build release binary
build:
    cargo build --release --locked

# Build debug binary
build-debug:
    cargo build --locked

# ── Linting ───────────────────────────────────

# Run clippy lints
clippy:
    cargo clippy --all-targets -- -D warnings

# Auto-fix clippy issues
clippy-fix:
    cargo clippy --all-targets --fix --allow-dirty --allow-staged -- -D warnings

# ── Formatting ────────────────────────────────

# Format all code
format:
    cargo fmt --all

# Check formatting (CI use)
format-check:
    cargo fmt --all -- --check

# ── Auto-fix (format + clippy) ───────────────

# Auto-fix everything
fix: clippy-fix format

# ── Testing ───────────────────────────────────

# Run all tests
test:
    cargo test

# Run tests with nextest (faster, better output)
test-nextest:
    cargo nextest run

# ── Security ──────────────────────────────────

# Run cargo-audit (dependency vulnerabilities)
audit:
    cargo audit

# Run cargo-deny (licenses + vulnerabilities)
deny:
    cargo deny check

# Full security check
security: audit deny

# ── Feature checks ────────────────────────────

# Check feature powerset compiles
features:
    cargo hack check --feature-powerset --locked

# ── Code coverage ─────────────────────────────

# Generate code coverage report (requires cargo-llvm-cov)
coverage:
    cargo llvm-cov --locked

# Generate code coverage as lcov
coverage-lcov:
    cargo llvm-cov --locked --lcov --output-path lcov.info

# ── Fuzz testing ──────────────────────────────

# Build fuzz targets
fuzz-build:
    cargo fuzz build

# Run fuzz targets (short)
fuzz-run:
    cargo fuzz run url_parser -- -max_total_time=60

# ── Typos check ───────────────────────────────

# Check for spelling mistakes
typos:
    cargo install typos-cli && typos

# ── Semver verification ───────────────────────

# Check semver compatibility (requires cargo-semver-checks)
semver-check:
    cargo semver-checks check-release

# ── Dependency freshness ──────────────────────

# Check for outdated dependencies
outdated:
    cargo outdated

# ── Maintenance ───────────────────────────────

# Update all dependencies
update:
    cargo update

# Clean build artifacts
clean:
    cargo clean

# Generate docs (no-deps for speed)
doc:
    cargo doc --no-deps

# ── Pre-commit ────────────────────────────────

# Run pre-commit checks
pre-commit: format-check clippy check

# ── Setup ─────────────────────────────────────

# Install tooling and configure git hooks
setup:
    rustup component add rustfmt clippy
    cargo install cargo-audit cargo-deny cargo-nextest cargo-hack cargo-semver-checks cargo-llvm-cov cargo-fuzz cargo-outdated typos-cli
    git config core.hooksPath .githooks
