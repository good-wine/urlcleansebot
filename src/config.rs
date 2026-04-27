use crate::shared::error::{AppError, AppResult};
use dotenvy::dotenv;
use std::env;

const DEFAULT_DATABASE_URL: &str = "sqlite:bot.db";
const DEFAULT_PORT: &str = "3000";
const DEFAULT_CLEARURLS_SOURCE: &str =
    "https://raw.githubusercontent.com/ClearURLs/Rules/refs/heads/master/data.min.json";
const DEFAULT_LIBREDIRECT_URL: &str =
    "https://raw.githubusercontent.com/libredirect/instances/main/data.json";
const DEFAULT_FARSIDE_URL: &str =
    "https://raw.githubusercontent.com/benbusby/farside/refs/heads/main/services-full.json";
const DEFAULT_AI_API_BASE: &str = "https://api.openai.com/v1";
const DEFAULT_AI_MODEL: &str = "gpt-3.5-turbo";
const DEFAULT_INLINE_MAX_RESULTS: usize = 5;

/// Configuration for the bot, loaded from environment variables.
#[derive(Clone, Debug)]
pub struct Config {
    pub bot_token: String,
    pub bot_username: String,
    pub database_url: String,
    pub server_addr: String,
    pub admin_id: i64,
    pub clearurls_source: String,
    pub libredirect_url: String,
    pub farside_url: String,
    pub ai_api_key: Option<String>,
    pub ai_api_base: String,
    pub ai_model: String,
    pub inline_max_results: usize,
    /// Public HTTPS URL where Telegram will POST updates.
    /// If unset, the bot runs in long-polling mode.
    pub webhook_url: Option<String>,
    /// Random secret used to verify the `X-Telegram-Bot-Api-Secret-Token`
    /// header on incoming webhook requests. Required when `webhook_url` is set.
    pub webhook_secret: Option<String>,
    /// TCP port the embedded HTTP server binds to in webhook mode.
    pub port: u16,
}

impl Config {
    /// Loads configuration from environment variables.
    ///
    /// # Errors
    /// Returns an error if required environment variables are missing.
    pub fn from_env() -> AppResult<Self> {
        dotenv().ok();

        let bot_token = env::var("TELOXIDE_TOKEN")
            .map_err(|_| AppError::Config("TELOXIDE_TOKEN deve essere impostato".to_string()))?;
        let mut bot_username = env::var("BOT_USERNAME")
            .map_err(|_| AppError::Config("BOT_USERNAME deve essere impostato".to_string()))?;
        if bot_username.starts_with('@') {
            bot_username = bot_username[1..].to_string();
        }
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            log::error!("DATABASE_URL non trovato, uso default.");
            DEFAULT_DATABASE_URL.to_string()
        });
        let port = env::var("PORT").unwrap_or_else(|_| {
            log::error!("PORT non trovato, uso default.");
            DEFAULT_PORT.to_string()
        });
        let server_addr = env::var("SERVER_ADDR").unwrap_or_else(|_| {
            log::error!("SERVER_ADDR non trovato, uso default.");
            format!("0.0.0.0:{}", port)
        });

        let admin_id = env::var("ADMIN_ID")
            .unwrap_or_else(|_| {
                log::error!("ADMIN_ID non trovato, uso '0'.");
                "0".to_string()
            })
            .parse()
            .unwrap_or_else(|_| {
                log::error!("ADMIN_ID non valido, uso 0.");
                0
            });

        let clearurls_source = env::var("CLEARURLS_SOURCE").unwrap_or_else(|_| {
            log::error!("CLEARURLS_SOURCE non trovato, uso default.");
            DEFAULT_CLEARURLS_SOURCE.to_string()
        });

        let libredirect_url = env::var("LIBREDIRECT_URL").unwrap_or_else(|_| {
            log::error!("LIBREDIRECT_URL non trovato, uso default.");
            DEFAULT_LIBREDIRECT_URL.to_string()
        });

        let farside_url = env::var("FARSIDE_URL").unwrap_or_else(|_| {
            log::error!("FARSIDE_URL non trovato, uso default.");
            DEFAULT_FARSIDE_URL.to_string()
        });

        let ai_api_key = env::var("AI_API_KEY").ok().filter(|s| !s.is_empty());
        let ai_api_base = env::var("AI_API_BASE").unwrap_or_else(|_| {
            log::error!("AI_API_BASE non trovato, uso default.");
            DEFAULT_AI_API_BASE.to_string()
        });
        let ai_model = env::var("AI_MODEL").unwrap_or_else(|_| {
            log::error!("AI_MODEL non trovato, uso default.");
            DEFAULT_AI_MODEL.to_string()
        });
        let inline_max_results = env::var("INLINE_MAX_RESULTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_INLINE_MAX_RESULTS)
            .min(50);

        let webhook_url = env::var("WEBHOOK_URL").ok().filter(|s| !s.is_empty());
        let webhook_secret = env::var("WEBHOOK_SECRET").ok().filter(|s| !s.is_empty());
        let port: u16 = env::var("PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        Ok(Self {
            bot_token,
            bot_username,
            database_url,
            server_addr,
            admin_id,
            clearurls_source,
            libredirect_url,
            farside_url,
            ai_api_key,
            ai_api_base,
            ai_model,
            inline_max_results,
            webhook_url,
            webhook_secret,
            port,
        })
    }

    /// Validates the configuration.
    ///
    /// # Errors
    /// Returns an error if validation fails.
    pub fn validate(&self) -> AppResult<()> {
        if self.bot_token.is_empty() || !self.bot_token.contains(':') {
            return Err(AppError::Config(
                "FATAL: TELOXIDE_TOKEN non è valido o è vuoto. Controlla il file .env".to_string(),
            ));
        }
        if self.bot_username.is_empty() {
            return Err(AppError::Config(
                "FATAL: BOT_USERNAME deve essere configurato".to_string(),
            ));
        }
        if self.inline_max_results == 0 {
            return Err(AppError::Config(
                "FATAL: INLINE_MAX_RESULTS deve essere maggiore di 0".to_string(),
            ));
        }

        // Render Reserved Ports check
        let reserved_ports = ["18012", "18013", "19099"];
        for port in reserved_ports {
            if self.server_addr.contains(port) {
                return Err(AppError::Config(format!(
                    "FATAL: La porta {} e' riservata da Render e non puo' essere usata.",
                    port
                )));
            }
        }

        // Webhook validation
        if let Some(url) = &self.webhook_url {
            if !url.starts_with("https://") {
                return Err(AppError::Config(
                    "FATAL: WEBHOOK_URL deve usare HTTPS (Telegram lo richiede).".to_string(),
                ));
            }
            let secret = self.webhook_secret.as_deref().unwrap_or("");
            if secret.is_empty() {
                return Err(AppError::Config(
                    "FATAL: WEBHOOK_SECRET e' obbligatorio quando WEBHOOK_URL e' impostato. \
                     Generalo con: openssl rand -hex 32"
                        .to_string(),
                ));
            }
            if secret.len() < 16 || secret.len() > 256 {
                return Err(AppError::Config(format!(
                    "FATAL: WEBHOOK_SECRET deve essere lungo 16-256 caratteri (attuale: {}).",
                    secret.len()
                )));
            }
            if !secret
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                return Err(AppError::Config(
                    "FATAL: WEBHOOK_SECRET puo' contenere solo A-Z a-z 0-9 _ - (regola di Telegram).".to_string()
                ));
            }
        }

        Ok(())
    }
}
