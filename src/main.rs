//! Main entry point for the ClearURLs Telegram Bot.
//!
//! Uses the extracted Telegram handler modules from `presentation/telegram`.
//! Clean Architecture skeleton is preserved in the application layer for future use.

use clear_urls_bot::{
    config::Config,
    db::Db,
    logging,
    presentation::telegram::handlers::run_bot,
    sanitizer::{AiEngine, RuleEngine},
};
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    logging::init_logging();
    tracing::info!("Avvio ClearURLs Bot");

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Configurazione non valida: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = config.validate() {
        tracing::error!("Validazione configurazione fallita: {}", e);
        std::process::exit(1);
    }

    let db = match Db::new(&config.database_url).await {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("Errore inizializzazione database: {}", e);
            std::process::exit(1);
        }
    };

    let rules = RuleEngine::new_lazy(&config.clearurls_source);
    if let Err(e) = rules.refresh().await {
        tracing::warn!(error = %e, "Impossibile scaricare le regole ClearURLs all'avvio, verranno scaricate al primo utilizzo");
    }
    let ai = AiEngine::new(&config);

    let (event_tx, _event_rx) = broadcast::channel::<serde_json::Value>(100);

    let bot = teloxide::Bot::new(&config.bot_token);

    tracing::info!("Bot avviato con user_id={}", config.admin_id);

    run_bot(bot, db, rules, ai, config, event_tx).await;
}
