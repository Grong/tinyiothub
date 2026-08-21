use async_trait::async_trait;
use tinyiothub_core::models::device::{Device, DeviceStats};
use tinyiothub_storage::Db;
use tinyiothub_storage::device::{DeviceStatusDistribution, QuickDevice};

use super::query::DeviceQueryService;
use tinyiothub_core::error::Result;

/// SQLite implementation of DeviceQueryService
#[derive(Debug, Clone)]
pub struct SqliteDeviceQueryService {
    db: Db,
}

impl SqliteDeviceQueryService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DeviceQueryService for SqliteDeviceQueryService {
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>> {
        self.db.search_devices(keyword, limit).await
    }

    async fn get_stats(&self) -> Result<DeviceStats> {
        self.db.device_stats_overview().await
    }

    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>> {
        self.db.count_devices_by_type().await
    }

    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>> {
        self.db.count_devices_by_driver().await
    }

    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>> {
        self.db.device_tree(root_id).await
    }

    async fn get_device_status_distribution(&self, workspace_id: Option<&str>) -> Result<DeviceStatusDistribution> {
        self.db.device_status_distribution(workspace_id).await
    }

    async fn get_quick_devices_list(&self, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickDevice>> {
        self.db.quick_devices(limit, workspace_id).await
    }
}
