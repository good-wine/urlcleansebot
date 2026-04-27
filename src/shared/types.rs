//! Common types and utilities.

use chrono::{DateTime, Utc};

/// Type alias for timestamps.
pub type Timestamp = DateTime<Utc>;

/// Utility function to get current timestamp.
pub fn now() -> Timestamp {
    Utc::now()
}

/// Pagination parameters.
#[derive(Debug, Clone)]
pub struct Pagination {
    pub page: usize,
    pub page_size: usize,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 50,
        }
    }
}

impl Pagination {
    pub fn offset(&self) -> usize {
        (self.page - 1) * self.page_size
    }

    pub fn limit(&self) -> usize {
        self.page_size
    }
}
