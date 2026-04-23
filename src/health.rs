// Health check endpoint for monitoring and load balancing
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub database: DatabaseStatus,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStatus {
    pub connected: bool,
    pub response_time_ms: Option<u64>,
}

pub struct HealthCheck {
    start_time: SystemTime,
    version: String,
}

impl HealthCheck {
    pub fn new(version: &str) -> Self {
        Self {
            start_time: SystemTime::now(),
            version: version.to_string(),
        }
    }

    pub fn uptime(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.start_time)
            .unwrap_or_default()
            .as_secs()
    }

    pub async fn check(&self, db: &crate::db::Db) -> Result<HealthStatus> {
        let uptime = self.uptime();
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        // Check database connectivity
        let db_status = self.check_database(db).await;

        let status = if db_status.connected {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };

        Ok(HealthStatus {
            status,
            version: self.version.clone(),
            uptime_seconds: uptime,
            database: db_status,
            timestamp,
        })
    }

    async fn check_database(&self, db: &crate::db::Db) -> DatabaseStatus {
        let start = std::time::Instant::now();

        // Simple ping query
        let result = sqlx::query("SELECT 1").fetch_one(&db.pool).await;

        let response_time = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => DatabaseStatus {
                connected: true,
                response_time_ms: Some(response_time),
            },
            Err(e) => {
                tracing::error!("Database health check failed: {}", e);
                DatabaseStatus {
                    connected: false,
                    response_time_ms: None,
                }
            }
        }
    }

    /// Simple liveness check (always returns true if service is running)
    pub fn liveness(&self) -> bool {
        true
    }

    /// Readiness check (returns true if service can handle requests)
    pub async fn readiness(&self, db: &crate::db::Db) -> bool {
        self.check_database(db).await.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check_creation() {
        let hc = HealthCheck::new("1.0.0");
        assert_eq!(hc.version, "1.0.0");
        assert!(hc.uptime() < 1);
    }

    #[test]
    fn test_liveness() {
        let hc = HealthCheck::new("1.0.0");
        assert!(hc.liveness());
    }

    #[tokio::test]
    async fn test_health_status_serialization() {
        let status = HealthStatus {
            status: "healthy".to_string(),
            version: "1.0.0".to_string(),
            uptime_seconds: 3600,
            database: DatabaseStatus {
                connected: true,
                response_time_ms: Some(5),
            },
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("1.0.0"));
    }
}
