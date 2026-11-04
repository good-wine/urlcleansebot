# URLCleanseBot — OpenCode Agent Instructions

## Quick Start
- `make check-fast` — quick syntax check (2-3s)
- `make build-fast` — debug build
- `make test` — run all tests
- `make clippy` — run linter
- `make fmt` — format code
- `just ci` — full CI pipeline locally
- `just build` — release build

## Build & CI Pipeline (exact order)
1. `make fmt` (or `cargo fmt --all -- --check`)
2. `make check-fast` (`cargo check --release`)
3. `make clippy` (`cargo clippy --all-targets -- -D warnings`)
4. `make test` (`cargo test`)
5. `cargo audit` + `cargo deny check`

**CI runs:** `fmt → check → clippy → test` (`.github/workflows/ci.yml`)

## Project Structure
- **App:** `src/main.rs` — entrypoint, config loading, bot startup
- **Core:** `src/lib.rs` — pub modules: `presentation`, `shared`, `config`, `db`, `i18n`, `logging`, `metrics`, `redirects`, `sanitizer`
- **DB:** `src/db/` — sqlx with `Any` (SQLite default, PostgreSQL optional)
- **Sanitizer:** `src/sanitizer/` — rule engine + optional AI engine
- **Config:** `src/config.rs` — loaded from `.env`; `AI_API_KEY` opt-in, default `false`
- **Telegram:** `src/presentation/telegram/` — bot commands and dispatcher
- **CI/CD:** `.github/workflows/` — fmt/check/lint/test, release, supply-chain, docker, dependabot

## Telegram Dispatcher Fix (critical)
- `dispatch_command()` now accepts `msg_id: MessageId` parameter
- `.reply_parameters(ReplyParameters::new(msg_id))` added to:
  - Rate limit exceeded message ("Troppe richieste. Attendi qualche secondo.")
  - Command execution error (`cmd_exec_error`)
- Messages now thread replying to the user's original command
- Call site in `handlers/message.rs` passes `msg_clone.id`

## Key Commands (non-obvious)
```bash
# Validate config
cargo run --release -- check-fast  # or just make check-fast

# Run tests only
cargo test                          # all tests
cargo test sanitizer                # sanitizer tests only

# Build release
make release                        # full optimized build

# Verify SBOM/signatures (release pipeline)
cosign verify-blob <registry>/urlcleansebot@<digest>
syft dir:. -o spdx-json=sbom.spdx.json  # source SBOM

# Pre-commit hook (already installed via git config)
git commit                          # runs cargo check/fmt/clippy automatically

# just shortcuts
just fmt      # cargo fmt --all -- --check
just fix    # auto-fix clippy + format
```

## Environment
- Copy `.env.example` → `.env` and fill values
- Required: `TELOXIDE_TOKEN`, `AI_API_KEY` (opt-in, default `false`)
- Copy `.env` from existing deployment

## Recent Fixes
- **Telegram reply threading**: Added `reply_parameters` to rate limit and error messages so bot responses thread back to user commands
- **Clippy format! args**: Fixed across `entropy.rs`, `normalize.rs`, `rule_engine/`, `security.rs` — change `format!("{}", var)` → `format!("{var}")`
- **Sanitizer tests**: Fixed `normalize_is_idempotent` regex and `sanitizer_preserves_path_and_host` case handling

## Residual Risks (known)
- AI API dependency (opt-in, fail-open fallback to rules)
- Single-instance VPS deployment model
- OpenAI cost monitoring requirements

## Next Steps (from completed pipeline)
1. ADR review — architecture governance cycle
2. AI eval enablement — activate with `AI_API_KEY` + `user_config.ai_enabled`
3. First chaos test — validate runbooks in staging
4. Production deployment — proceed with green verify
5. 30-day SLO review — burn-rate monitoring & adjustment

## Verification Commands
```bash
# Eval harness gate (blocks CI if thresholds not met)
cd evals/ai-sanitizer/harness
cargo run --release -- gate --config config.toml --result result.json --output gate_result.md

# Check SLO burn-rate
otelcli query 'sum(rate(urlcleansebot_slo_errors[5m])) by (component)

# List ADRs
ls docs/adr/

# Run all verification make targets
make check-fast && make clippy && make test
```
