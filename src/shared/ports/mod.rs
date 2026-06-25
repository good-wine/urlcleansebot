pub mod sanitizer;
pub mod database;
pub mod ai;
pub mod redirect;

pub use database::DatabasePort;
pub use sanitizer::SanitizerService;
pub use ai::AiProvider;
pub use redirect::RedirectProvider;

// Re-export mockall mocks for integration tests
#[cfg(any(test, feature = "test-utils"))]
pub use database::MockDatabasePort;
#[cfg(any(test, feature = "test-utils"))]
pub use sanitizer::MockSanitizerService;
#[cfg(any(test, feature = "test-utils"))]
pub use ai::MockAiProvider;
#[cfg(any(test, feature = "test-utils"))]
pub use redirect::MockRedirectProvider;
