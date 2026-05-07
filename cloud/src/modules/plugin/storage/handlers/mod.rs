//! 存储处理器

use async_trait::async_trait;

use crate::{modules::plugin::storage::StorageData, shared::error::Error};

#[async_trait]
pub trait StorageHandler: Send + Sync {
    async fn write(&self, data: &StorageData) -> Result<(), Error>;
    fn name(&self) -> &str;
}

pub mod influxdb;
pub mod postgres;

pub use influxdb::InfluxdbHandler;
pub use postgres::PostgresHandler;
