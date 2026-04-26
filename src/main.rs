//! Main entry point using Clean Architecture.

use clear_urls_bot::{
    application::{
        commands::handlers::*,
        queries::handlers::*,
    },
    domain::repositories::*,
    infrastructure::repositories::*,
    presentation::telegram::*,
    shared::error::*,
    config::Config,
    logging,
};
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::Bot;
use teloxide::macros::BotCommands;

#[tokio::main]
async fn main() -> AppResult<()> {
    logging::init_logging();
    tracing::info!("Avvio ClearURLs Bot con Clean Architecture");

    let config = Config::from_env()?;
    config.validate()?;

    // Initialize infrastructure layer
    let pool = sqlx::PgPool::connect(&config.database_url).await?;

    // Initialize repositories
    let user_repo = Arc::new(PostgresUserRepository::new(pool.clone()));
    let url_history_repo = Arc::new(PostgresUrlHistoryRepository::new(pool.clone()));
    let whitelist_repo = Arc::new(PostgresWhitelistRepository::new(pool.clone()));
    let statistics_repo = Arc::new(PostgresStatisticsRepository::new(pool.clone()));

    // Initialize command handlers
    let clean_url_handler = Arc::new(CleanUrlCommandHandlerImpl::new(
        url_history_repo.clone() as Arc<dyn UrlHistoryRepository>,
        whitelist_repo.clone() as Arc<dyn WhitelistRepository>,
    ));
    let update_user_prefs_handler = Arc::new(UpdateUserPreferencesCommandHandlerImpl::new(user_repo.clone() as Arc<dyn UserRepository>));
    let update_user_lang_handler = Arc::new(UpdateUserLanguageCommandHandlerImpl::new(user_repo.clone() as Arc<dyn UserRepository>));
    let manage_whitelist_handler = Arc::new(ManageWhitelistCommandHandlerImpl::new(whitelist_repo.clone() as Arc<dyn WhitelistRepository>));

    // Initialize query handlers
    let get_user_profile_handler = Arc::new(GetUserProfileQueryHandlerImpl::new(user_repo.clone() as Arc<dyn UserRepository>));
    let get_global_stats_handler = Arc::new(GetGlobalStatisticsQueryHandlerImpl::new(statistics_repo.clone() as Arc<dyn StatisticsRepository>));
    let get_whitelist_handler = Arc::new(GetWhitelistQueryHandlerImpl::new(whitelist_repo.clone() as Arc<dyn WhitelistRepository>));

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
        .branch(
            Update::filter_message()
                .endpoint(handle_url_cleaning),
        );

    // Start bot with dependency injection
    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![app_services])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

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
