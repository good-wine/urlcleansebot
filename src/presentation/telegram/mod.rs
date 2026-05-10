//! Telegram bot presentation layer.
//!
//! Contains all Telegram-specific handlers, helpers, and UI logic.
//! Organized into submodules for better maintainability.

pub mod command_dispatcher;
pub mod commands;
pub mod handlers;
pub mod helpers;
pub mod security_scan;
pub mod settings;

// Re-export main handlers for external use
pub use handlers::run_bot;
