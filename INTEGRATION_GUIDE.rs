//! Integration Guide for Refactored Modules
//!
//! This file demonstrates how to use the new command handlers and utility modules
//! in the refactored ClearURLs Bot codebase.

// Example 1: Using the command handlers module
#[cfg(test)]
mod command_handler_examples {
    use teloxide::prelude::*;
    use crate::presentation::telegram::commands;
    use crate::i18n;

    #[tokio::test]
    async fn example_handle_stats() {
        // Usage example:
        // let bot = Bot::new("YOUR_TOKEN");
        // let chat_id = ChatId(123456);
        // let user_id = 789;
        // let tr = i18n::get_translations("it");
        //
        // if let Err(e) = commands::handle_stats(&bot, chat_id, user_id, &db, &config, &tr).await {
        //     eprintln!("Error: {}", e);
        // }
    }
}

// Example 2: Using URL processor module
#[cfg(test)]
mod url_processor_examples {
    use crate::shared::url_processor;
    use crate::sanitizer::RuleEngine;

    #[tokio::test]
    async fn example_process_url() {
        // Usage example:
        // let rules = RuleEngine::new_lazy("https://rules2.clearurls.xyz/data.minify.json");
        // let url = "https://example.com?utm_source=test&foo=bar";
        //
        // if let Some(cleaned_info) = url_processor::process_single_url(
        //     url,
        //     &rules,
        //     &[], // custom_rules
        //     &[], // ignored_domains
        // ).await {
        //     println!("Original: {}", cleaned_info.original_url);
        //     println!("Cleaned: {}", cleaned_info.cleaned_url);
        //     println!("Removed params: {}", cleaned_info.params_removed);
        // }
    }

    #[tokio::test]
    async fn example_format_url() {
        use crate::shared::url_processor;
        
        // Safe URL formatting
        let safe_url = "https://example.com";
        let formatted = url_processor::format_url_for_display(safe_url);
        // Result: <a href="https://example.com">https://example.com</a>

        // Potentially unsafe URL (handled gracefully)
        let unsafe_url = "javascript:alert('xss')";
        let formatted = url_processor::format_url_for_display(unsafe_url);
        // Result: <code>javascript:alert('xss')</code>
    }

    #[tokio::test]
    async fn example_deduplicate() {
        use crate::shared::url_processor;

        let urls = vec![
            "https://example.com".to_string(),
            "https://example.com".to_string(), // duplicate
            "https://other.com".to_string(),
        ];

        let unique = url_processor::deduplicate_urls(urls);
        assert_eq!(unique.len(), 2); // Duplicates removed
    }
}

// Example 3: Using validation module
#[cfg(test)]
mod validation_examples {
    use crate::shared::validation;

    #[test]
    fn example_validate_url() {
        match validation::validate_url("https://example.com") {
            Ok(url) => println!("Valid URL: {}", url),
            Err(e) => eprintln!("Invalid URL: {}", e),
        }
    }

    #[test]
    fn example_validate_domain() {
        match validation::validate_domain("example.com") {
            Ok(domain) => println!("Valid domain: {}", domain),
            Err(e) => eprintln!("Invalid domain: {}", e),
        }
    }

    #[test]
    fn example_validate_language_code() {
        // Valid
        assert!(validation::validate_language_code("it").is_ok());
        assert!(validation::validate_language_code("en").is_ok());

        // Invalid
        assert!(validation::validate_language_code("Italian").is_err());
        assert!(validation::validate_language_code("i").is_err());
    }

    #[test]
    fn example_sanitize_html() {
        let unsafe_html = r#"<script>alert('xss')</script><p>Safe content</p>"#;
        let safe_html = validation::sanitize_html_content(unsafe_html);
        assert!(!safe_html.contains("<script>"));
    }

    #[test]
    fn example_detect_phishing() {
        // Philshing-like content
        assert!(validation::detect_suspicious_content("Please confirm your password"));
        assert!(validation::detect_suspicious_content("Urgent action required"));

        // Normal content
        assert!(!validation::detect_suspicious_content("Hello, how are you?"));
    }
}

// Example 4: Integration in handlers
#[cfg(test)]
mod handler_integration_examples {
    use crate::presentation::telegram::commands;
    use crate::shared::url_processor;
    use crate::shared::validation;

    // This is how you would refactor handle_message to use these new modules:

    /*
    pub async fn handle_message_refactored(
        bot: Bot,
        msg: Message,
        db: crate::db::Db,
        rules: RuleEngine,
        config: crate::config::Config,
    ) -> Result<(), RequestError> {
        let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
        let chat_id = msg.chat.id;
        let msg_text = msg.text().map(|t| t.to_string()).unwrap_or_default();

        // Validate input
        let msg_safe = validation::sanitize_html_content(&msg_text);

        // ... other logic ...

        // Handle commands
        if msg_text.starts_with("/stats") {
            let tr = i18n::get_translations(&lang_code);
            commands::handle_stats(&bot, chat_id, user_id, &db, &user_config, &tr).await?;
            return Ok(());
        }

        if msg_text.starts_with("/help") {
            let tr = i18n::get_translations(&lang_code);
            commands::handle_help(&bot, chat_id, &tr).await?;
            return Ok(());
        }

        // ... rest of logic ...
    }
    */
}

// Example 5: Error handling patterns
#[cfg(test)]
mod error_handling_examples {
    use crate::shared::error::AppError;

    fn example_url_validation_error_handling() {
        // Pattern 1: Using map_err for clean error propagation
        let result = async {
            Ok::<String, AppError>("https://example.com".to_string())
        };

        // Pattern 2: Using context for debugging
        match result {
            Ok(url) => println!("URL: {}", url),
            Err(e) => eprintln!("Error with context: {}", e),
        }
    }
}

/// Checklist for using the new modules:
///
/// 1. **Command Handlers**
///    - [x] Import from `crate::presentation::telegram::commands`
///    - [x] Each command is now a standalone `async fn`
///    - [x] All handlers return `CommandResult` (alias for Result<(), RequestError>)
///
/// 2. **URL Processor**
///    - [x] Import from `crate::shared::url_processor`
///    - [x] Use `process_single_url()` for individual URL cleaning
///    - [x] Use `format_url_for_display()` for safe HTML rendering
///    - [x] Use `deduplicate_urls()` to remove duplicates
///
/// 3. **Validation**
///    - [x] Import from `crate::shared::validation`
///    - [x] Call validation functions before processing user input
///    - [x] Use `AppError` for consistent error reporting
///    - [x] Always sanitize HTML content from external sources
///
/// 4. **Testing New Modules**
///    - [x] Each module includes unit tests
///    - [x] Run tests with `cargo test --lib`
///    - [x] Add integration tests in `tests/` directory
///
/// 5. **Performance Considerations**
///    - [x] URL processor functions are pure (no I/O)
///    - [x] Validation is O(n) where n is input length
///    - [x] Use caching for repeated URL validation results
