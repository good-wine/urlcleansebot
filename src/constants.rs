//! Application-wide constants.
//!
//! Each constant includes a comment explaining its origin, purpose, and constraints.
//!
//! # Categories
//!
//! - Telegram limits
//! - Caching
//! - Rate limiting
//! - Data retention
//! - Security scans
//! - URL expansion
// ── Telegram limits ───────────────────────────────────────────────────────

/// Maximum message length allowed by Telegram (4096 UTF-8 characters).
/// Source: <https://core.telegram.org/bots/api#sending-messages>
pub const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;

/// Safety margin below Telegram's hard limit to avoid truncation edge cases.
/// Gives 96 chars of headroom for HTML tags and formatting.
pub const MAX_RESPONSE_LENGTH: usize = 4000;

/// Maximum URL length accepted for processing (prevents DoS via huge URLs).
pub const MAX_URL_LENGTH: usize = 2048;

/// Maximum length for inline query results.
pub const MAX_INLINE_RESULTS: usize = 50;

// ── Caching ───────────────────────────────────────────────────────────────

/// Maximum number of URL cleaning results cached in-process (moka cache).
/// Each entry stores original → cleaned URL mapping.
/// 10k entries ≈ ~2-5 MB depending on URL length.
pub const URL_CACHE_MAX_CAPACITY: u64 = 10_000;

/// TTL for callback query deduplication cache (seconds).
/// Prevents replay attacks: if the same callback ID is seen twice within
/// this window, the second request is silently ignored.
pub const CALLBACK_DEDUP_TTL_SECS: u64 = 300;

/// TTL for LibRedirect/Farside catalog cache (seconds).
/// Upstream catalogs change infrequently; 6 hours is a safe default.
pub const CATALOG_CACHE_TTL_SECS: u64 = 21_600;

// ── Rate limiting ─────────────────────────────────────────────────────────

/// Maximum requests per user before rate limiting kicks in.
pub const RATE_LIMIT_REQUESTS: u32 = 10;

/// Time window for rate limiting (seconds).
pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

// ── Data retention ────────────────────────────────────────────────────────

/// Default number of days to retain cleaned link history.
/// Users can request earlier deletion via /clear_history.
/// Set via DATA_RETENTION_DAYS env var (default: 90).
pub const DEFAULT_DATA_RETENTION_DAYS: i64 = 90;

// ── Security scans ────────────────────────────────────────────────────────

/// Polling interval for VirusTotal analysis results (milliseconds).
/// Free tier: 4 requests/minute, so we poll conservatively.
pub const VIRUSTOTAL_POLL_INTERVAL_MS: u64 = 1200;

/// Polling interval for URLScan.io analysis results (milliseconds).
pub const URLSCAN_POLL_INTERVAL_MS: u64 = 1500;

/// Maximum number of alternative frontend instances to suggest per URL.
pub const MAX_FRONTEND_SUGGESTIONS: usize = 3;

// ── URL expansion ─────────────────────────────────────────────────────────

/// Maximum number of redirects to follow when expanding shortened URLs.
/// Prevents infinite redirect loops.
pub const MAX_REDIRECT_FOLLOW: usize = 10;

// ── Activity progress bar ────────────────────────────────────────────────

/// Maximum activity level shown in the progress bar (10 segments).
pub const ACTIVITY_MAX_LEVEL: usize = 10;

/// Threshold for activity level calculation (cleaned_count / ACTIVITY_THRESHOLD).
pub const ACTIVITY_THRESHOLD: i64 = 10;

/// Cap cleaned_count for activity bar calculation to avoid overflow.
pub const ACTIVITY_COUNT_CAP: i64 = 100;
