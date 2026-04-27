//! Main entry point using Clean Architecture.

use clear_urls_bot::{
    application::{commands::handlers::*, queries::handlers::*},
    config::Config,
    domain::repositories::*,
    infrastructure::repositories::*,
    logging,
    presentation::telegram::*,
    redirects::RedirectService,
    sanitizer::RuleEngine,
    shared::error::*,
};
use std::sync::Arc;
use teloxide::error_handlers::LoggingErrorHandler;
use teloxide::macros::BotCommands;
use teloxide::prelude::*;
use teloxide::update_listeners::webhooks;
use teloxide::Bot;

#[tokio::main]
async fn main() -> AppResult<()> {
    logging::init_logging();
    tracing::info!("Avvio ClearURLs Bot con Clean Architecture");

    let config = Config::from_env()?;
    config.validate()?;

    // Initialize infrastructure layer
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .idle_timeout(std::time::Duration::from_secs(300))
        .max_lifetime(std::time::Duration::from_secs(1800))
        .connect(&config.database_url)
        .await?;

    // Initialize rule engine
    let rule_engine = Arc::new(RuleEngine::new_lazy(&config.clearurls_source));

    // Initialize redirect service
    let redirect_service =
        RedirectService::from_config(&config.libredirect_url, &config.farside_url)
            .expect("Failed to initialize redirect service");

    // Initialize repositories
    let user_repo = Arc::new(PostgresUserRepository::new(pool.clone()));
    let url_history_repo = Arc::new(PostgresUrlHistoryRepository::new(pool.clone()));
    let whitelist_repo = Arc::new(PostgresWhitelistRepository::new(pool.clone()));
    let statistics_repo = Arc::new(PostgresStatisticsRepository::new(pool.clone()));

    // Initialize command handlers
    let clean_url_handler = Arc::new(CleanUrlCommandHandlerImpl::new(
        url_history_repo.clone() as Arc<dyn UrlHistoryRepository>,
        whitelist_repo.clone() as Arc<dyn WhitelistRepository>,
        rule_engine.clone(),
        redirect_service,
    ));
    let update_user_prefs_handler = Arc::new(UpdateUserPreferencesCommandHandlerImpl::new(
        user_repo.clone() as Arc<dyn UserRepository>,
    ));
    let update_user_lang_handler = Arc::new(UpdateUserLanguageCommandHandlerImpl::new(
        user_repo.clone() as Arc<dyn UserRepository>,
    ));
    let manage_whitelist_handler = Arc::new(ManageWhitelistCommandHandlerImpl::new(
        whitelist_repo.clone() as Arc<dyn WhitelistRepository>,
    ));

    // Initialize query handlers
    let get_user_profile_handler = Arc::new(GetUserProfileQueryHandlerImpl::new(
        user_repo.clone() as Arc<dyn UserRepository>
    ));
    let get_global_stats_handler = Arc::new(GetGlobalStatisticsQueryHandlerImpl::new(
        statistics_repo.clone() as Arc<dyn StatisticsRepository>,
    ));
    let get_whitelist_handler = Arc::new(GetWhitelistQueryHandlerImpl::new(
        whitelist_repo.clone() as Arc<dyn WhitelistRepository>
    ));

    // Create application services container
    let app_services = AppServices::new(
        clean_url_handler,
        update_user_prefs_handler,
        update_user_lang_handler,
        manage_whitelist_handler,
        get_user_profile_handler,
        get_global_stats_handler,
        get_whitelist_handler,
    );

    // Initialize Telegram bot
    let bot = Bot::new(&config.bot_token);

    // Set up bot commands
    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<ClearUrlsBotCommand>()
                .endpoint(handle_commands),
        )
        .branch(Update::filter_message().endpoint(handle_url_cleaning));

    // Start bot with dependency injection
    if let Some(webhook_url) = &config.webhook_url {
        // Webhook mode
        tracing::info!("Avvio in modalità webhook: {}", webhook_url);

        let webhook_secret = config.webhook_secret.as_ref().ok_or_else(|| {
            AppError::Config(
                "WEBHOOK_SECRET è richiesto quando WEBHOOK_URL è impostato".to_string(),
            )
        })?;

        let addr = config
            .server_addr
            .parse()
            .map_err(|_| AppError::Config("SERVER_ADDR non valido".to_string()))?;

        // Parse webhook URL
        let parsed_url = url::Url::parse(webhook_url)
            .map_err(|e| AppError::Config(format!("WEBHOOK_URL non valido: {}", e)))?;

        // Create webhook options
        let opts = webhooks::Options::new(addr, parsed_url).secret_token(webhook_secret.clone());

        // Create webhook listener
        let listener = webhooks::axum(bot.clone(), opts)
            .await
            .map_err(|e| AppError::Config(format!("Errore creazione webhook listener: {}", e)))?;

        // Start dispatcher with webhook listener
        let error_handler = LoggingErrorHandler::with_custom_text("Errore webhook listener");
        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![app_services])
            .enable_ctrlc_handler()
            .build()
            .dispatch_with_listener(listener, error_handler)
            .await;
    } else {
        // Long-polling mode
        tracing::info!("Avvio in modalità long-polling");
        Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![app_services])
            .enable_ctrlc_handler()
            .build()
            .dispatch()
            .await;
    }

    Ok(())
}

/// Bot commands enum.
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum ClearUrlsBotCommand {
    Start,
    Stats,
    Whitelist,
    Settings,
}

/// Handle bot commands.
async fn handle_commands(
    bot: Bot,
    msg: Message,
    cmd: ClearUrlsBotCommand,
    services: AppServices,
) -> AppResult<()> {
    match cmd {
        ClearUrlsBotCommand::Start => handle_start(bot, msg, services).await?,
        ClearUrlsBotCommand::Stats => handle_stats(bot, msg, services).await?,
        ClearUrlsBotCommand::Whitelist => handle_whitelist(bot, msg, services).await?,
        ClearUrlsBotCommand::Settings => handle_settings(bot, msg, services).await?,
    }

    Ok(())
}
