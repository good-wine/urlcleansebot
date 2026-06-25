//! Application-wide constants.

pub const TELEGRAM_MAX_MESSAGE_LENGTH: usize = 4096;
pub const MAX_RESPONSE_LENGTH: usize = 4000;
pub const MAX_URL_LENGTH: usize = 2048;
pub const MAX_INLINE_RESULTS: usize = 50;

pub const URL_CACHE_MAX_CAPACITY: u64 = 10_000;
pub const CALLBACK_DEDUP_TTL_SECS: u64 = 300;
pub const CATALOG_CACHE_TTL_SECS: u64 = 21_600;

pub const DEFAULT_DATA_RETENTION_DAYS: i64 = 90;

pub const VIRUSTOTAL_POLL_INTERVAL_MS: u64 = 1200;
pub const URLSCAN_POLL_INTERVAL_MS: u64 = 1500;
pub const MAX_FRONTEND_SUGGESTIONS: usize = 3;

pub const MAX_REDIRECT_FOLLOW: usize = 10;

pub const ACTIVITY_MAX_LEVEL: usize = 10;
pub const ACTIVITY_THRESHOLD: i64 = 10;
pub const ACTIVITY_COUNT_CAP: i64 = 100;

/// Parameter names that are affiliate/referral marketing — NOT tracking.
/// These are how creators get paid for recommendations.
/// Honor Creator mode preserves these.
pub const AFFILIATE_PARAMS: &[&str] = &[
    "tag", "ref", "ref_", "referrer", "click", "affiliate", "aff", "aff_id",
    "siteID", "campaign", "campaignid", "ad_id", "siteid", "ascsubtag",
    "linkId", "linkid", "mkevt", "campid", "mkrid", "toolid", "pub",
    "partner", "partnerid", "sid", "s_kwcid", "cjevent", "afsrc",
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
    "utm_id", "wickedid", "irgwc", "ext_id",
];

/// Parameter names that are ONLY affiliate (not also tracking).
/// These are safe to preserve in Honor Creator mode.
pub const SAFE_AFFILIATE_PARAMS: &[&str] = &[
    "tag", "ref", "ref_", "referrer", "affiliate", "aff", "aff_id",
    "siteID", "ascsubtag", "linkId", "linkid", "mkevt", "campid",
    "mkrid", "toolid", "pub", "partner", "partnerid", "sid",
    "cjevent", "irgwc", "ext_id",
];

/// Known URL shortening services for expansion.
pub const SHORTENER_DOMAINS: &[&str] = &[
    "bit.ly", "tinyurl.com", "t.co", "goo.gl", "rebrand.ly", "buff.ly",
    "is.gd", "ow.ly", "t.me", "shorturl.at", "amzn.to", "a.co", "geni.us",
    "rstyle.me", "click.linksynergy.com",
];

/// Common tracking parameters removed by aggressive fallback.
pub const AGGRESSIVE_TRACKERS: &[&str] = &[
    "gs_lcrp", "oq", "sourceid", "client", "bih", "biw", "ved", "ei",
    "iflsig", "adgrpid", "nw", "matchtype",
];

/// Additional URL shortener regex patterns (covers services not in SHORTENER_DOMAINS).
pub const SHORTENER_REGEX_PATTERNS: &[&str] = &[
    r"(?i)^https?://[^/]+\.[^/]{2,3}/[a-zA-Z0-9]{4,12}$",
];
