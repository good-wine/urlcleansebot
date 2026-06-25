use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts, register_counter, register_gauge, register_histogram};

use std::sync::LazyLock;

macro_rules! counter {
    ($name:expr, $help:expr) => {
        LazyLock::new(|| {
            register_counter!(Opts::new($name, $help)).expect("Failed to register counter")
        })
    };
}

macro_rules! gauge {
    ($name:expr, $help:expr) => {
        LazyLock::new(|| {
            register_gauge!(Opts::new($name, $help)).expect("Failed to register gauge")
        })
    };
}

macro_rules! histogram {
    ($name:expr, $help:expr, $buckets:expr) => {
        LazyLock::new(|| {
            register_histogram!(HistogramOpts::new($name, $help).buckets($buckets))
                .expect("Failed to register histogram")
        })
    };
}

// ── Counters ─────────────────────────────────────────────────

pub static REQUESTS_MESSAGE: LazyLock<Counter> =
    counter!("urlcleansebot_requests_message_total", "Total messages processed");

pub static REQUESTS_INLINE: LazyLock<Counter> =
    counter!("urlcleansebot_requests_inline_total", "Total inline queries processed");

pub static REQUESTS_CALLBACK: LazyLock<Counter> =
    counter!("urlcleansebot_requests_callback_total", "Total callback queries processed");

pub static REQUESTS_EDITED: LazyLock<Counter> =
    counter!("urlcleansebot_requests_edited_total", "Total edited messages processed");

pub static SANITIZATIONS_CLEANED: LazyLock<Counter> =
    counter!("urlcleansebot_sanitizations_cleaned_total", "Total URLs cleaned successfully");

pub static SANITIZATIONS_UNCHANGED: LazyLock<Counter> =
    counter!("urlcleansebot_sanitizations_unchanged_total", "Total URLs with no changes");

pub static SANITIZATIONS_REDIRECTED: LazyLock<Counter> =
    counter!("urlcleansebot_sanitizations_redirected_total", "Total URLs redirected");

pub static SANITIZATIONS_BLOCKED: LazyLock<Counter> =
    counter!("urlcleansebot_sanitizations_blocked_total", "Total URLs blocked");

pub static RATE_LIMIT_HITS: LazyLock<Counter> =
    counter!("urlcleansebot_rate_limit_hits_total", "Total rate limit hits");

pub static ERRORS_TOTAL: LazyLock<Counter> =
    counter!("urlcleansebot_errors_total", "Total errors encountered");

pub static REDIRECT_LOOKUPS: LazyLock<Counter> =
    counter!("urlcleansebot_redirect_lookups_total", "Total alternative frontend lookups");

pub static AI_SANITIZATIONS: LazyLock<Counter> =
    counter!("urlcleansebot_ai_sanitizations_total", "Total AI-powered sanitizations");

// ── Histograms ───────────────────────────────────────────────

pub static SANITIZATION_DURATION: LazyLock<Histogram> = histogram!(
    "urlcleansebot_sanitization_duration_seconds",
    "Time spent in sanitization pipeline",
    vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0]
);

pub static HTTP_REQUEST_DURATION: LazyLock<Histogram> = histogram!(
    "urlcleansebot_http_request_duration_seconds",
    "Time spent on external HTTP requests",
    vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
);

// ── Gauges ───────────────────────────────────────────────────

pub static ACTIVE_SANITIZATIONS: LazyLock<Gauge> =
    gauge!("urlcleansebot_active_sanitizations", "Currently ongoing sanitizations");

pub static DB_CONNECTIONS: LazyLock<Gauge> =
    gauge!("urlcleansebot_db_connections", "Current database connection count");

// ── Rendering ────────────────────────────────────────────────

pub fn render_prometheus() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&prometheus::gather(), &mut buffer) {
        tracing::error!(error = %e, "Failed to encode Prometheus metrics");
        return String::new();
    }

    String::from_utf8_lossy(&buffer).to_string()
}

// ── Convenience helpers ─────────────────────────────────────

pub fn inc_sanitization(matched: bool, redirected: bool, blocked: bool) {
    if blocked {
        SANITIZATIONS_BLOCKED.inc();
    } else if redirected {
        SANITIZATIONS_REDIRECTED.inc();
    } else if matched {
        SANITIZATIONS_CLEANED.inc();
    } else {
        SANITIZATIONS_UNCHANGED.inc();
    }
}
