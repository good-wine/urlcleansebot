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
- **CI/CD:** `.github/workflows/` — fmt/check/lint/test, release, supply-chain, docker, dependabot

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
- Required: `TELOXIDE_TOKEN`, `BOT_USERNAME`
- Optional: `DATABASE_URL` (defaults to sqlite:bot.db), `AI_API_KEY` (opt-in, privacy controls in `docs/ai-governance/`)
- `APP_ENV=development` → pretty logs; `production` → structured JSON logs
- `RUST_LOG=url_cleanse_bot=info,teloxide=info,axum=info`

## Conventional Commits
- **Scope list:** `sanitizer|db|telegram|ai|config|deploy|docs|deps|metrics|i18n|security|redirects|bench|release`
- **Format:** `<type>(<scope>): <subject>` — first line only, no uppercase start
- **Hook:** `.githooks/commit-msg` validates locally (install: `git config core.hooksPath .githooks`)
- **CI:** `commit-lint.yml` + `pr-title.yml` enforce on PR
- **Auto-merge:** `auto-merge.yml` merges Dependabot PRs when CI green + conventional commit + label `dependencies` + `automerge`
- **See:** `CONTRIBUTING.md` §Conventional Commits, `docs/GIT_WORKFLOW.md`

## Testing Quirks
- **Integration tests** require services (wiremock, etc.) — see test files for prerequisites
- **Unit tests:** `cargo test` runs all; 17/17 pass in evals harness
- **Run specific test:** `cargo test sanitizer::urls`
- **Fixtures/fixtures:** edge cases in `evals/ai-sanitizer/dataset/`

## Architecture Notes (not obvious from filenames)
- **DB choice ADR:** SQLite default for single-instance VPS; PostgreSQL optional via `sqlx::Any` (see `docs/adr/0001-database-choice-sqlite-vs-postgresql.md`)
- **Sanitization layered pipeline:** rules → entropy → aggressive → AI fallback → honor creator → normalize (see `docs/adr/0002-sanitization-strategy-rule-based-entropy-ai.md`)
- **AI engine opt-in:** `AI_API_KEY` + `user_config.ai_enabled` default `false`; fail-open to rule-based pipeline (see `docs/adr/0003-ai-engine-optional-openai-compatible.md`)
- **Distroless deployment:** multi-stage Docker build, health checks, resource limits, rolling updates (see `docs/adr/0004-deployment-model-docker-compose-distroless.md`)
- **Observability:** OpenTelemetry OTLP traces + Prometheus metrics + JSON logs; 8 SLI, 7 SLO, 23 PrometheusRule alerts (see `docs/adr/0005-observability-opentelemetry-logging-metrics.md`)
- **AI Governance:** Model card, DPIA, risk-tier (Minimal/Art.50 transparency only), inventory.yaml — all in `docs/ai-governance/` (see `docs/ai-governance/`)

## Governance & Compliance
- **AI Act:** Minimal Risk classification; Art. 50 transparency (badge 🤖 in UI, settings, `/help`); no training, only inference on external API
- **Supply Chain:** SLSA Level 2, SBOMs (SPDX + CycloneDX source + container), Cosign keyless signing, Rekor transparency, `cargo-audit` + `cargo-deny`
- **Runbooks (9):** `docs/runbooks/` — matched to alerts (db-pool-exhausted, ai-high-error-rate, ai-latency-spike, rule-fetch-failure, high-error-rate, high-latency, oom-kill, container-restart, high-memory)
- **Chaos testing:** 11 scenarios in `chaos/` — validate runbooks in staging before prod

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