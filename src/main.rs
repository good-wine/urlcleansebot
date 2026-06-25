//! Main entry point for URLCleanseBot.
//!
//! Uses the extracted Telegram handler modules from `presentation/telegram`.
//! Clean Architecture skeleton is preserved in the application layer for future use.

use url_cleanse_bot::{
    config::Config,
    db::Db,
    logging,
    presentation::telegram::handlers::run_bot,
    sanitizer::{AiEngine, RuleEngine, linkumori::LinkumoriEngine},
};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    logging::init_logging();
    tracing::info!("Starting URLCleanseBot");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Invalid configuration: {}", e);
            std::process::exit(1);
        },
    };

    if let Err(e) = config.validate() {
        tracing::error!("Configuration validation failed: {}", e);
        std::process::exit(1);
    }

    let db = match Db::new(&config.database_url).await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Database initialization error: {}", e);
            std::process::exit(1);
        },
    };

    let rules = RuleEngine::new_lazy(&config.clearurls_source);
    if let Err(e) = rules.refresh().await {
        tracing::warn!(error = %e, "Could not download ClearURLs rules at startup, will load on first use");
    }
    let ai = AiEngine::new(&config);

    let linkumori = LinkumoriEngine::new();
    if let Ok(source_url) = std::env::var("LINKUMORI_SOURCE") {
        let linkumori_source = url_cleanse_bot::sanitizer::linkumori::LinkumoriSource {
            url: source_url,
            priority: 0,
            is_override: false,
        };
        if let Err(e) = linkumori.add_source(&linkumori_source).await {
            tracing::warn!(error = %e, "Could not load Linkumori rules");
        }
    }

    let (event_tx, _event_rx) = broadcast::channel::<serde_json::Value>(100);

    let bot = teloxide::Bot::new(&config.bot_token);

    tracing::info!("URLCleanseBot started with user_id={}", config.admin_id);

    run_bot(bot, db, rules, ai, linkumori, config, event_tx).await;
}
