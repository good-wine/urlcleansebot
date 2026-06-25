use std::env;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initializes the logging and tracing system.
///
/// Two modes based on `APP_ENV`:
/// - `development` (default): Pretty-printed, colored console output.
/// - `production`: JSON-formatted logs for structured ingestion.
pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("url_cleanse_bot=info,teloxide=info,axum=info"));

    let env = env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

    let registry = Registry::default()
        .with(env_filter)
        .with(ErrorLayer::default());

    if env == "production" {
        let json_layer = fmt::layer().json().with_thread_ids(true).with_target(true);
        registry.with(json_layer).init();
    } else {
        let fmt_layer = fmt::layer()
            .pretty()
            .with_thread_ids(true)
            .with_target(true);
        registry.with(fmt_layer).init();
    }

    tracing::info!(env = %env, "Sistema di logging inizializzato");
}

/// Shutdown tracing resources (placeholder for OpenTelemetry integration).
pub fn shutdown_tracing() {
    tracing::info!("Tracing system shut down");
}
