# Git Workflow per URLCleanseBot

Questa guida documenta il workflow Git standardizzato per il progetto URLCleanseBot, basato su **Conventional Commits** e **Semantic Versioning**.

---

## 📋 Conventional Commits

Ogni commit deve seguire il formato [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Tipi di commit (`type`)

| Tipo | Descrizione | Version Bump |
|------|-------------|--------------|
| `feat` | Nuova funzionalità | **minor** |
| `fix` | Correzione bug | **patch** |
| `docs` | Solo documentazione | *nessuno* |
| `refactor` | Refactoring (né fix né feature) | *nessuno* |
| `test` | Aggiunta/miglioramento test | *nessuno* |
| `chore` | Build, CI, tooling, dipendenze | *nessuno* |
| `perf` | Miglioramento performance | **patch** (o **minor** se breaking) |
| `security` | Fix sicurezza | **patch** |
| `ci` | Cambiamenti CI/CD | *nessuno* |
| `build` | Sistema di build, dipendenze esterne | *nessuno* |
| `revert` | Revert di un commit precedente | *dipende* |
| `style` | Formattazione, niente logica | *nessuno* |

### Scope (`scope`) — Opzionale ma raccomandato

| Scope | Area |
|-------|------|
| `sanitizer` | Logica di sanitizzazione URL |
| `db` | Database, migrazioni, query |
| `telegram` | Bot Telegram, handler, middleware |
| `ai` | AI/ML, embedding, classificazione |
| `config` | Configurazione, variabili ambiente |
| `deploy` | Deployment, infrastruttura |
| `docs` | Documentazione |
| `deps` | Aggiornamento dipendenze |
| `metrics` | Metriche, observability |
| `i18n` | Internazionalizzazione |
| `security` | Sicurezza, hardening |
| `redirects` | Gestione redirect |
| `bench` | Benchmark |
| `release` | Processo di release |

### Breaking Changes

Due modi equivalenti per indicare un breaking change (**major version bump**):

1. **Exclamation mark** nel type: `feat(sanitizer)!: rewrite parser`
2. **Footer** `BREAKING CHANGE:` nel corpo del commit:

```
feat(sanitizer): add new tracking parameter rule

BREAKING CHANGE: the `Sanitizer::new()` signature changed, now requires a config object
```

### Esempi

```bash
# Feature con scope
feat(sanitizer): add support for TikTok tracking parameters

# Bug fix
fix(db): resolve connection leak in pool on high load

# Documentazione
docs: update README with new environment variables

# Performance improvement
perf(sanitizer): optimize regex compilation cache

# Breaking change (major)
refactor(telegram)!: migrate to teloxide 0.18 API

# Con footer breaking change
fix(ai): handle empty embedding response

BREAKING CHANGE: `EmbeddingClient::embed` now returns `Result<Vec<f32>, Error>` instead of `Vec<f32>`

# Chore / dependencies
chore(deps): update tokio to 1.38

# Test
test(sanitizer): add property-based tests for URL parsing

# CI
ci: add commitlint workflow for PR validation
```

---

## 🌿 Branching Strategy

### Trunk-Based Development con Short-Lived Branches

```
main (protected)
  │
  ├── feature/nuova-funzionalita     (da main, merge via PR)
  ├── fix/correzione-bug             (da main, merge via PR)
  ├── chore/aggiorna-dipendenze      (da main, merge via PR)
  └── release/v1.2.0                 (solo maintainer, per hotfix)
```

### Regole

1. **`main` è protetto** — richiede PR review + CI verde
2. **Branch naming**: `<type>/<breve-descrizione>` (es: `feat/add-redirect-handler`)
3. **Una PR = una feature/fix** — niente "mixed" PR
4. **Rebase su main** prima del merge (squash merge)
5. **Cancella branch** dopo merge

---

## 🚀 Release Process

### Release Automatica (Standard)

1. **Merge PR su `main`** → triggera `semantic-release` job
2. **Analisi commit** da ultimo tag → determina bump (patch/minor/major)
3. **Generazione changelog** con `git-cliff` → prepend a `CHANGELOG.md`
4. **Creazione tag** `v<version>` + push
5. **GitHub Release** con note generate
6. **Pipeline build** (binary, Docker, SBOM, firme) triggerata dal tag push

### Release Manuale (Maintainer)

```bash
# Opzione 1: Via GitHub UI
# Actions → Release → Run workflow → scegli patch/minor/major/exact

# Opzione 2: Via CLI (richiede permessi)
gh workflow run release.yml -f version=minor

# Opzione 3: Versione esatta
gh workflow run release.yml -f version=exact -f exact_version=v2.0.0
```

### Hotfix Process

Per fix critici su versione già rilasciata:

```bash
# 1. Crea branch hotfix dal tag di produzione
git checkout -b hotfix/v1.2.1 v1.2.0

# 2. Applica fix (commit conventional: fix(...): ...)
# 3. Push e apri PR verso main
git push origin hotfix/v1.2.1

# 4. Dopo merge su main, la release automatica creerà v1.2.1 (patch)
#    oppure forza versione esatta se necessario
```

---

## 🔧 Setup Locale

### Pre-commit Hook (Opzionale ma Raccomandato)

Il progetto include un hook `commit-msg` in `.githooks/` per validazione locale:

```bash
# Attiva gli hook locali (una volta)
git config core.hooksPath .githooks

# Oppure usa just (già configurato)
just setup
```

L'hook valida il messaggio prima del commit. Se fallisce, correggi il messaggio e riprova.

### Commit con Messaggio Guidato

Usa `git commit` senza `-m` per aprire l'editor con template, oppure:

```bash
# Commitizen (se installato)
cz commit

# Oppure scrivi manualmente seguendo il formato
git commit -m "feat(sanitizer): add support for new tracking param"
```

---

## ✅ Checklist PR

Prima di aprire una PR:

- [ ] Commit seguono Conventional Commits
- [ ] Messaggi chiari e imperativi (`add` non `added`)
- [ ] Scope appropriato (vedi tabella sopra)
- [ ] Breaking changes documentati (`!` o `BREAKING CHANGE:`)
- [ ] Test passano (`cargo test`)
- [ ] Lint pulito (`cargo clippy -- -D warnings`)
- [ ] Formattazione corretta (`cargo fmt --check`)
- [ ] Documentazione aggiornata se necessario

---

## 🛠️ Tooling

| Tool | Scopo |
|------|-------|
| `commitlint` | Validazione CI dei commit message |
| `git-cliff` | Generazione changelog da commit |
| `cargo-semver-checks` | Verifica compatibilità API Rust |
| `cosign` | Firma artefatti (SLSA) |
| `cargo-cyclonedx` | Generazione SBOM |

---

## 📚 Riferimenti

- [Conventional Commits Spec](https://www.conventionalcommits.org/)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)
- [git-cliff Documentation](https://github.com/orhun/git-cliff)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)