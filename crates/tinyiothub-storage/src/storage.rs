//! Unified storage facade.
//!
//! Replaces `cloud::application::DataContext` as the data access entry point.
//! Provides database connection pool, in-memory caches, and repository access.

use sqlx::SqlitePool;

use crate::cache::DeviceCache;
use tinyiothub_core::error::{Error, Result};

/// Storage layer entry point.
#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
    device_cache: DeviceCache,
}

impl Storage {
    /// Create a new Storage instance with an existing pool.
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let storage = Self {
            pool,
            device_cache: DeviceCache::new(),
        };

        // TODO: init device_cache from repository
        // storage.init_device_cache().await?;

        Ok(storage)
    }

    /// Create a new Storage instance from a database URL.
    pub async fn from_url(url: &str) -> Result<Self> {
        let pool = sqlx::SqlitePool::connect(url)
            .await
            .map_err(|e| Error::DatabaseError(e.to_string()))?;
        Self::new(pool).await
    }

    /// Access the raw database pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Access the device cache.
    pub fn device_cache(&self) -> &DeviceCache {
        &self.device_cache
    }

    /// Consume self to get the underlying pool.
    pub fn into_pool(self) -> SqlitePool {
        self.pool
    }
}
