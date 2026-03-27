//! 存储处理器

use async_trait::async_trait;
use tracing::debug;

use crate::domain::plugin::storage::StorageData;
use crate::shared::error::Error;

#[async_trait]
pub trait StorageHandler: Send + Sync {
    async fn write(&self, data: &StorageData) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod postgres;
pub mod influxdb;

pub use postgres::PostgresHandler;
pub use influxdb::InfluxdbHandler;
