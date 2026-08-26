use async_trait::async_trait;
use tinyiothub_core::models::thing::{Thing, ThingStats};
use tinyiothub_storage::Db;
use tinyiothub_storage::thing::{QuickThing, ThingStatusDistribution};

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
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Thing>> {
        self.db.search_things(keyword, limit).await
    }

    async fn get_stats(&self) -> Result<ThingStats> {
        self.db.thing_stats_overview().await
    }

    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>> {
        self.db.count_things_by_type().await
    }

    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>> {
        self.db.count_things_by_driver().await
    }

    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Thing>> {
        self.db.thing_tree(root_id).await
    }

    async fn get_device_status_distribution(&self, workspace_id: Option<&str>) -> Result<ThingStatusDistribution> {
        self.db.thing_status_distribution(workspace_id).await
    }

    async fn get_quick_devices_list(&self, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickThing>> {
        self.db.quick_things(limit, workspace_id).await
    }
}
