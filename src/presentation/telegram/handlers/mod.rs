pub mod callback;
pub mod inline;
pub mod message;

use crate::metrics;
use crate::redirects::RedirectService;
use crate::sanitizer::{AiEngine, RuleEngine, linkumori::LinkumoriEngine};
use teloxide::dispatching::Dispatcher;
use teloxide::prelude::*;
use teloxide::types::Update;
use tracing;

// Re-export handler functions for the public API
pub use callback::handle_callback;
pub use inline::{handle_chosen_inline_result, handle_inline_query};
pub use message::{handle_edited_message, handle_message};

pub async fn run_bot(
    bot: Bot,
    db: crate::db::Db,
    rules: RuleEngine,
    ai: AiEngine,
    linkumori: LinkumoriEngine,
    config: crate::config::Config,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    shutdown_token: tokio_util::sync::CancellationToken,
) {
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_edited_message().endpoint(handle_edited_message))
        .branch(Update::filter_inline_query().endpoint(handle_inline_query))
        .branch(Update::filter_chosen_inline_result().endpoint(handle_chosen_inline_result))
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    let webhook_url = config.webhook_url.clone();
    let webhook_secret = config.webhook_secret.clone();
    let port = config.port;

    let redirect_service =
        match RedirectService::from_config(&config.libredirect_url, &config.farside_url) {
            Ok(svc) => svc,
            Err(e) => {
                tracing::error!("Impossibile inizializzare RedirectService: {e}");
                return;
            },
        };

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![
            db.clone(),
            rules,
            ai,
            linkumori,
            config,
            event_tx,
            redirect_service
        ])
        .enable_ctrlc_handler()
        .build();

    match webhook_url {
        Some(url) => {
            use teloxide::update_listeners::webhooks;
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let parsed = match url::Url::parse(&url) {
                Ok(u) => u,
                Err(e) => {
                    tracing::error!("WEBHOOK_URL non valido ({url}): {e}");
                    return;
                },
            };
            let mut opts = webhooks::Options::new(addr, parsed);
            if let Some(secret) = webhook_secret {
                opts = opts.secret_token(secret);
            }
            tracing::info!("Avvio in modalita' WEBHOOK: bind={addr}, public_url={url}");

            let (listener, shutdown_future, telegram_router) =
                match webhooks::axum_to_router(bot, opts).await {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("Impossibile avviare il webhook: {e}");
                        return;
                    },
                };

            let health_db = db.clone();
            let health_router = axum::Router::new()
                .route("/health", axum::routing::get(health_liveness))
                .route(
                    "/ready",
                    axum::routing::get({
                        let health_db = health_db.clone();
                        move || health_readiness(health_db)
                    }),
                )
                .route("/metrics", axum::routing::get(metrics_handler));

            let app = telegram_router.merge(health_router);

            let server = axum::serve(
                tokio::net::TcpListener::bind(addr)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("Impossibile bind alla porta {}: {e}", port);
                        std::process::exit(1);
                    }),
                app,
            )
            .with_graceful_shutdown(shutdown_future);

            tokio::select! {
                _ = dispatcher.dispatch_with_listener(
                    listener,
                    teloxide::error_handlers::LoggingErrorHandler::with_custom_text(
                        "Errore webhook listener",
                    ),
                ) => {},
                result = server => {
                    if let Err(e) = result {
                        tracing::error!("Server webhook terminato con errore: {e}");
                    }
                }
                _ = shutdown_token.cancelled() => {
                    tracing::info!("Shutdown signal received, stopping webhook mode");
                }
            }
        },
        None => {
            tracing::info!("Avvio in modalita' LONG-POLLING");
            tokio::select! {
                _ = dispatcher.dispatch() => {},
                _ = shutdown_token.cancelled() => {
                    tracing::info!("Shutdown signal received, stopping long-polling mode");
                }
            }
        },
    }
}

async fn metrics_handler() -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::OK, metrics::render_prometheus())
}

async fn health_liveness() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

async fn health_readiness(db: crate::db::Db) -> axum::http::StatusCode {
    match db.ping().await {
        Ok(_) => axum::http::StatusCode::OK,
        Err(e) => {
            tracing::error!("Health check readiness fallito: {e}");
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        },
    }
}
