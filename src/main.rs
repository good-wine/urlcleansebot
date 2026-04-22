use clear_urls_bot::{
    bot,
    config::Config,
    db::Db,
    logging,
    sanitizer::{AiEngine, RuleEngine},
};
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::Bot;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::interval;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init_logging();
    tracing::info!("Avvio ClearURLs Bot");

    let config = Config::from_env()?;
    config.validate()?;

    let db = Db::new(&config.database_url).await?;
    let rules = RuleEngine::new_lazy(&config.clearurls_source);
    let ai = AiEngine::new(&config);

    log_optional_feature("VirusTotal", "VIRUSTOTAL_API_KEY", "VIRUSTOTAL_ALERT_ONLY");
    log_optional_feature("URLScan.io", "URLSCAN_API_KEY", "URLSCAN_ALERT_ONLY");

    // Reqwest client with longer timeout for Telegram long-polling
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let bot = Bot::with_client(&config.bot_token, client);

    // Real-time event channel (used by bot internally)
    let (event_tx, _) = tokio::sync::broadcast::channel::<serde_json::Value>(100);

    // Async-friendly panic notification: forward panics through a tokio channel
    // instead of spinning up a new runtime inside a panic hook (which is unreliable
    // on serverless containers that may be killed seconds after the panic).
    let admin_id = config.admin_id;
    let (panic_tx, mut panic_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        panic_hook(info);
        let _ = panic_tx.send(format!("[PANIC] {info}"));
    }));
    let bot_for_panic = bot.clone();
    tokio::spawn(async move {
        while let Some(msg) = panic_rx.recv().await {
            tracing::error!("{msg}");
            if admin_id != 0 {
                let _ = bot_for_panic.send_message(ChatId(admin_id), msg).await;
            }
        }
    });

    let bot_task = tokio::spawn(bot::run_bot(
        bot,
        db.clone(),
        rules.clone(),
        ai,
        config.clone(),
        event_tx.clone(),
    ));

    let rules_refresh = rules.clone();
    let refresh_task = tokio::spawn(async move {
        if let Err(e) = rules_refresh.refresh().await {
            tracing::error!("Errore nel download iniziale delle regole: {e}");
        }
        let mut tick = interval(Duration::from_secs(86400));
        tick.tick().await;
        loop {
            tick.tick().await;
            if let Err(e) = rules_refresh.refresh().await {
                tracing::error!("Errore durante l'aggiornamento delle regole: {e}");
            }
        }
    });

    // Graceful shutdown: intercept SIGTERM (sent by Leapcell/K8s before stopping
    // the container) and SIGINT (Ctrl-C in dev) so we close DB pool cleanly.
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = sigterm.recv() => tracing::info!("SIGTERM ricevuto, shutdown pulito"),
        _ = sigint.recv()  => tracing::info!("SIGINT ricevuto, shutdown pulito"),
        res = bot_task => match res {
            Ok(_) => tracing::info!("Task bot terminato normalmente"),
            Err(e) => tracing::error!("Task bot terminato con errore: {e:?}"),
        },
        res = refresh_task => match res {
            Ok(_) => tracing::info!("Task refresh terminato normalmente"),
            Err(e) => tracing::error!("Task refresh terminato con errore: {e:?}"),
        },
    }

    db.pool.close().await;
    tracing::info!("Shutdown completato");
    Ok(())
}

fn log_optional_feature(feature: &str, key_var: &str, alert_only_var: &str) {
    let configured = std::env::var(key_var)
        .map(|v| !v.is_empty() && !v.contains("your_"))
        .unwrap_or(false);

    if !configured {
        tracing::info!("⚠️  {feature}: DISABILITATO (API key non configurata)");
        return;
    }

    let alert_only = std::env::var(alert_only_var)
        .ok()
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true);

    if alert_only {
        tracing::info!("✅ {feature}: ABILITATO (modalità SOLO ALLERTA)");
    } else {
        tracing::info!("✅ {feature}: ABILITATO (modalità report completa)");
    }
}
