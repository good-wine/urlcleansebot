//! Security middleware and helpers for ClearURLs Bot

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Simple per-user rate limiter (in-memory, not persistent)
pub struct RateLimiter {
    users: Mutex<HashMap<i64, Instant>>,
    min_interval: Duration,
}

impl RateLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            min_interval,
        }
    }

    /// Returns true if allowed, false if rate-limited.
    pub fn check(&self, user_id: i64) -> bool {
        let mut users = match self.users.lock() {
            Ok(u) => u,
            Err(poisoned) => {
                // Recover from a poisoned mutex rather than panicking.
                log::error!("RateLimiter mutex poisoned, recovering: {poisoned}");
                poisoned.into_inner()
            }
        };
        let now = Instant::now();
        match users.get(&user_id) {
            Some(last) if now.duration_since(*last) < self.min_interval => false,
            _ => {
                users.insert(user_id, now);
                true
            }
        }
    }
}

/// Global rate limiter instance (1 request/sec per user)
pub static RATE_LIMITER: Lazy<RateLimiter> =
    Lazy::new(|| RateLimiter::new(Duration::from_secs(1)));

/// Sanitise arbitrary user-supplied text (inline queries, command arguments,
/// etc.).  Strips control characters and enforces a length cap.  This
/// intentionally does **not** validate URL format — use
/// [`sanitizer::validation::is_valid_url`] separately when a URL is required.
pub fn sanitize_input(input: &str) -> String {
    let mut s: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if s.len() > 4_000 {
        // Truncate at a character boundary.
        s = s.chars().take(4_000).collect();
    }
    s
}

/// Sanitise callback query data (not a URL — just safe ASCII trimming).
pub fn sanitize_callback(input: &str) -> String {
    let mut s: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    if s.len() > 4_000 {
        s = s.chars().take(4_000).collect();
    }
    s
}

/// Checks if a user is admin.
pub fn is_admin(user_id: i64, admin_id: i64) -> bool {
    user_id == admin_id
}
