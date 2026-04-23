# Fixes Applied to ClearURLs Bot

## 1. `src/db/implementation.rs` — Non-existent `connect_options()` call

### Problem
`Pool<Any>` in sqlx 0.8 does **not** expose a `connect_options()` method that
returns an object with a `.database_url.scheme()` getter.  Every call to

```rust
self.pool.connect_options().database_url.scheme() == "sqlite"
```

would fail to compile (and did fail at `cargo check`).  The pattern appeared in
`init()` and `get_stats_by_day()`.

### Fix
Store the raw database URL string in the `Db` struct and derive the backend
type from it:

```rust
// Before
#[derive(Clone)]
pub struct Db {
    pub pool: Pool<Any>,
}

// After
#[derive(Clone)]
pub struct Db {
    pub pool: Pool<Any>,
    database_url: String,   // ← new private field
}
```

`Db::new()` now stores `database_url` and a new private helper replaces every
occurrence of the broken call:

```rust
fn is_sqlite(&self) -> bool {
    self.database_url.starts_with("sqlite")
}
```

All SQLite/PostgreSQL branch selections in `init()` and `get_stats_by_day()`
now call `self.is_sqlite()`.

---

## 2. `src/security.rs` — `sanitize_input` rejected all non-URL text

### Problem
The original implementation delegated to `is_valid_url()` and returned an
empty string for any input that did not start with `http://` or `https://`:

```rust
pub fn sanitize(input: &str) -> String {
    let mut s = input.trim().replace(|c: char| c.is_control(), "");
    if s.len() > 4000 { s.truncate(4000); }
    if !is_valid_url(&s) {          // ← silently drops e.g. Telegram inline queries
        log::error!("Input non valido: {}", s);
        return String::new();
    }
    s
}
```

`sanitize_input` is called in `src/bot.rs` for **inline query text**
(`q.query.trim()`) which is never a bare URL.  This caused every inline query
to be silently discarded.

### Fix
`sanitize_input` now only strips control characters and caps length — URL
validation is a separate concern:

```rust
pub fn sanitize_input(input: &str) -> String {
    let mut s: String = input.trim().chars().filter(|c| !c.is_control()).collect();
    if s.len() > 4_000 { s = s.chars().take(4_000).collect(); }
    s
}
```

---

## 3. `src/security.rs` — Mutex `.unwrap()` could panic

### Problem
```rust
let mut users = self.users.lock().unwrap();
```
If another thread panicked while holding the lock, this `unwrap()` would
propagate the panic to every subsequent caller.

### Fix
Recover gracefully from a poisoned mutex:

```rust
let mut users = match self.users.lock() {
    Ok(u) => u,
    Err(poisoned) => {
        log::error!("RateLimiter mutex poisoned, recovering");
        poisoned.into_inner()
    }
};
```

---

## 4. `src/sanitizer/validation.rs` — Mutex `.unwrap()` + incorrect cache logic

### Problem
```rust
let mut cache = URL_CACHE.lock().unwrap();    // panic-prone
if let Some(&result) = cache.get(url) { return result; }
let result = url.starts_with("http://") || url.starts_with("https://");
cache.insert(url.to_string(), result);
result
```
The `unwrap()` could panic.  Also, the value was computed *before* the lock was
released, but the early-return path still held the lock unnecessarily.

### Fix
Compute the result first (no lock needed), then update the cache in a
best-effort, panic-free way:

```rust
pub fn is_valid_url(url: &str) -> bool {
    let result = url.starts_with("http://") || url.starts_with("https://");
    if let Ok(mut cache) = URL_CACHE.lock() {
        cache.entry(url.to_string()).or_insert(result);
    }
    result
}
```

---

## Files changed

| File | Change |
|------|--------|
| `src/db/implementation.rs` | Add `database_url: String` to `Db`; add `is_sqlite()` helper; replace all `connect_options()` calls |
| `src/security.rs` | `sanitize_input` no longer validates URL format; poison-safe mutex recovery |
| `src/sanitizer/validation.rs` | Compute result before locking; panic-free cache update |
