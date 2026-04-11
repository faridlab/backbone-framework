//! Database connection management

use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

/// Database connection manager
pub struct ConnectionManager {
    pool: PgPool,
}

impl ConnectionManager {
    /// Create new connection pool
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await?;

        Ok(Self { pool })
    }

    /// Get connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Close all connections
    pub async fn close(&self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }

    /// Test database connection
    pub async fn test_connection(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}