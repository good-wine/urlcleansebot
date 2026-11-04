# 0001 - Database Choice: SQLite vs PostgreSQL for Single-Instance VPS

**Status**: Accepted

**Date**: 2026-11-04

## Context

URLCleanseBot is a self-hosted Telegram bot deployed as a single Docker container on a VPS. It needs to persist:
- User configurations (enabled state, AI settings, mode, ignored domains, privacy mode)
- Custom rules per user
- Cleaned link history (for analytics/debugging)
- Chat configurations (for group settings)

The bot uses `sqlx::Any` to abstract over the database driver, allowing both SQLite and PostgreSQL at runtime via the `DATABASE_URL` environment variable.

### Requirements
- **Single-instance deployment**: No horizontal scaling, no replication needs
- **Low operational overhead**: Self-hosted users may lack database administration experience
- **Backup simplicity**: Single-file backup preferred
- **Resource constraints**: VPS with 512MB RAM limit (per compose.yml)
- **Migration support**: Schema evolution via embedded SQLx migrations
- **Concurrency**: Low write throughput (user config updates, link logging), moderate read throughput

## Decision

**Default to SQLite (`sqlite:bot.db?mode=rwc`) as the primary database for single-instance deployments. Support PostgreSQL as an optional alternative for users who need it.**

### Rationale

| Factor | SQLite | PostgreSQL |
|--------|--------|------------|
| **Operational complexity** | Zero (single file) | Requires separate process, config, auth |
| **Resource usage** | ~few MB RAM | ~50-100MB base + connections |
| **Backup** | `cp bot.db bot.db.backup` | `pg_dump`, WAL archiving |
| **Migration support** | Full (sqlx migrations work) | Full |
| **Concurrency** | WAL mode handles bot workload | Better for high concurrency |
| **Portability** | Single file, no dependencies | Requires running server |

For a Telegram bot with <100 req/s typical load, SQLite in WAL mode (`mode=rwc`) provides sufficient concurrency. The `sqlx::Any` abstraction means users can switch to PostgreSQL by changing `DATABASE_URL` (e.g., `postgres://user:pass@host/db`) without code changes.

### Implementation Details

1. **Default**: `sqlite:bot.db?mode=rwc` (WAL mode, read-write-create)
2. **Connection pool**: `max_connections=5`, `min_connections=1` (configured in `Db::new`)
3. **Migrations**: Embedded via `sqlx::migrate!()` — run on startup in `Db::init()`
4. **Schema**: Dual DDL in `Db::init()` with `IF NOT EXISTS` for both dialects
5. **Backup**: Document `cp bot.db bot.db.$(date +%F).bak` in README

## Consequences

### Positive
- **Zero-config deployment**: Works out of the box with Docker volume mount
- **Trivial backup/restore**: Copy the file; no tools needed
- **Low memory footprint**: Fits comfortably in 512MB container limit
- **Portability**: Database file moves with the container

### Negative
- **No remote access**: Cannot connect external tools without SSH/file copy
- **Limited concurrency**: Writer blocks readers during write (WAL mitigates)
- **No built-in replication**: Not suitable for multi-instance (out of scope)
- **Schema changes**: `ALTER TABLE` limitations in SQLite (workaround: recreate table)

## Alternatives Considered

1. **PostgreSQL only**: Rejected — adds operational burden for single-instance use case
2. **SQLite only (no PostgreSQL support)**: Rejected — `sqlx::Any` makes dual support nearly free; some users prefer PostgreSQL for tooling
3. **Embedded key-value (sled, redb)**: Rejected — SQL migrations and query flexibility needed for user configs

## Links

- [SQLite WAL mode documentation](https://www.sqlite.org/wal.html)
- [sqlx::Any documentation](https://docs.rs/sqlx/latest/sqlx/any/)
- [compose.yml volume mount](../compose.yml#L11)