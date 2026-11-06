use sqlx::{Error as SqlxError, SqlitePool};

use crate::config::DatabaseConfig;

/// Db abstraction layer for SQLx
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
    pub(crate) workspace_stores: crate::harness_workspace::WorkspaceStores,
    pub(crate) workspace_store_config: DatabaseConfig,
    pub(crate) workspace_store_root: std::path::PathBuf,
    #[cfg(test)]
    pub(crate) workspace_store_test_hooks: Option<std::sync::Arc<crate::harness_workspace::WorkspaceStoreTestHooks>>,
    #[cfg(test)]
    pub(crate) harness_transition_test_hooks:
        Option<std::sync::Arc<crate::harness_effect::tests::HarnessTransitionTestHooks>>,
}

impl Db {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            workspace_stores: Default::default(),
            workspace_store_config: DatabaseConfig::default(),
            workspace_store_root: crate::harness_workspace::default_workspace_store_root(),
            #[cfg(test)]
            workspace_store_test_hooks: None,
            #[cfg(test)]
            harness_transition_test_hooks: None,
        }
    }

    /// Point at an isolated workspace root while sharing the platform pool.
    /// Embedders and tests use this to keep Harness generations separate.
    pub fn with_workspace_store_root(mut self, root: std::path::PathBuf) -> Self {
        self.workspace_store_root = root;
        self
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

        Ok(Self {
            workspace_store_config: config.clone(),
            ..Self::new(pool)
        })
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
