use sqlx::{Error as SqlxError, SqlitePool};

use crate::config::DatabaseConfig;

/// Db abstraction layer for SQLx
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// One-step cloud entry: build the pool (FK pragma on), run embedded
    /// migrations (backup + FK-OFF connection + integrity enforcement),
    /// return the facade.
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, SqlxError> {
        tracing::info!("Creating database connection pool with config: {:?}", config);

        let pool = crate::pool::pool_options(config)
            .connect_with(crate::pool::connect_options(config)?)
            .await?;

        tracing::info!("Running database migrations...");
        crate::migrations::run_migrations(&pool).await?;
        tracing::info!("Database migrations completed successfully");

        Ok(Self::new(pool))
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Begin a transaction
    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, SqlxError> {
        self.pool.begin().await
    }

    /// 健康检查探活（SELECT 1；自 cloud health/service_manager 收编）。
    pub async fn ping(&self) -> Result<(), SqlxError> {
        sqlx::query("SELECT 1").fetch_optional(&self.pool).await?;
        Ok(())
    }
}
