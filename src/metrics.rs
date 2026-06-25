use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};

macro_rules! metric {
    ($name:ident) => {
        pub static $name: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(0));
    };
}

metric!(REQUESTS_MESSAGE);
metric!(REQUESTS_INLINE);
metric!(REQUESTS_CALLBACK);
metric!(REQUESTS_EDITED);
metric!(SANITIZATIONS_CLEANED);
metric!(SANITIZATIONS_UNCHANGED);
metric!(SANITIZATIONS_REDIRECTED);
metric!(SANITIZATIONS_BLOCKED);
metric!(RATE_LIMIT_HITS);
metric!(ERRORS_TOTAL);
metric!(REDIRECT_LOOKUPS);
metric!(AI_SANITIZATIONS);

pub fn render_prometheus() -> String {
    let pairs = [
        ("requests_message", &REQUESTS_MESSAGE, "Messages processed"),
        (
            "requests_inline",
            &REQUESTS_INLINE,
            "Inline queries processed",
        ),
        (
            "requests_callback",
            &REQUESTS_CALLBACK,
            "Callback queries processed",
        ),
        (
            "requests_edited",
            &REQUESTS_EDITED,
            "Edited messages processed",
        ),
        (
            "sanitizations_cleaned",
            &SANITIZATIONS_CLEANED,
            "URLs cleaned",
        ),
        (
            "sanitizations_unchanged",
            &SANITIZATIONS_UNCHANGED,
            "URLs unchanged",
        ),
        (
            "sanitizations_redirected",
            &SANITIZATIONS_REDIRECTED,
            "URLs redirected",
        ),
        (
            "sanitizations_blocked",
            &SANITIZATIONS_BLOCKED,
            "URLs blocked",
        ),
        ("rate_limit_hits", &RATE_LIMIT_HITS, "Rate limit hits"),
        ("errors_total", &ERRORS_TOTAL, "Total errors"),
        ("redirect_lookups", &REDIRECT_LOOKUPS, "Redirect lookups"),
        ("ai_sanitizations", &AI_SANITIZATIONS, "AI sanitizations"),
    ];

    let mut out = String::new();
    out.push_str("# HELP clearurls_bot metrics\n");
    out.push_str("# TYPE clearurls_bot gauge\n");
    for (name, counter, _help) in &pairs {
        let val = counter.load(Ordering::Relaxed);
        out.push_str(&format!("clearurls_bot_{name} {val}\n"));
    }
    out
}

pub fn inc_sanitization(matched: bool, redirected: bool, blocked: bool) {
    if blocked {
        SANITIZATIONS_BLOCKED.fetch_add(1, Ordering::Relaxed);
    } else if redirected {
        SANITIZATIONS_REDIRECTED.fetch_add(1, Ordering::Relaxed);
    } else if matched {
        SANITIZATIONS_CLEANED.fetch_add(1, Ordering::Relaxed);
    } else {
        SANITIZATIONS_UNCHANGED.fetch_add(1, Ordering::Relaxed);
    }
}
