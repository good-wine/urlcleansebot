//! Domain entities for the ClearURLs bot.
//!
//! These represent the core business concepts and rules.

/// Represents a user in the system.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: i64,
    pub username: Option<String>,
    pub language: Language,
    pub preferences: UserPreferences,
}

/// User language preference.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
#[derive(Default)]
pub enum Language {
    Italian,
    #[default]
    English,
    Spanish,
    French,
    German,
}

/// User preferences for bot behavior.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserPreferences {
    pub notifications_enabled: bool,
    pub ai_enabled: bool,
    pub privacy_mode: bool,
    pub action_mode: ActionMode,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            ai_enabled: false,
            privacy_mode: false,
            action_mode: ActionMode::Reply,
        }
    }
}

/// How the bot should handle cleaned URLs.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ActionMode {
    Reply,  // Reply with cleaned URL
    Delete, // Delete message and repost (admin only)
}

/// Represents a URL that needs to be cleaned.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlToClean {
    pub original_url: String,
    pub user_id: i64,
    pub chat_id: i64,
    pub message_id: Option<i32>,
}

/// Result of URL cleaning operation.
#[derive(Debug, Clone, PartialEq)]
pub struct CleaningResult {
    pub original_url: String,
    pub cleaned_url: Option<String>,
    pub tracking_params_removed: Vec<String>,
    pub security_warnings: Vec<SecurityWarning>,
    pub alternative_frontends: Vec<AlternativeFrontend>,
}

/// Security warning for a URL.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityWarning {
    pub severity: SecuritySeverity,
    pub source: SecuritySource,
    pub message: String,
}

/// Severity levels for security warnings.
#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySeverity {
    Clean,
    Suspicious,
    Malicious,
}

/// Source of security analysis.
#[derive(Debug, Clone, PartialEq)]
pub enum SecuritySource {
    VirusTotal,
    UrlScan,
}

/// Alternative frontend suggestion.
#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeFrontend {
    pub service: String,
    pub frontend: String,
    pub url: String,
    pub description: String,
}

/// Statistics for a user.
#[derive(Debug, Clone, PartialEq)]
pub struct UserStatistics {
    pub user_id: i64,
    pub total_cleaned: i64,
    pub total_warnings: i64,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// Global system statistics.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalStatistics {
    pub total_users: usize,
    pub total_urls_cleaned: usize,
}

/// URL cleaning history entry.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlHistory {
    pub user_id: i64,
    pub original_url: String,
    pub cleaned_url: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
