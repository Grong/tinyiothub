// Device query service trait — migrated from domain/device/query_service.rs

use tinyiothub_core::models::device::{Device, DeviceStats};
use async_trait::async_trait;
use crate::{
    modules::monitoring::types::{DeviceStatusDistribution, QuickDevice},
    shared::error::Result,
};

#[async_trait]
pub trait DeviceQueryService: Send + Sync {
    async fn search(&self, keyword: &str, limit: Option<u32>) -> Result<Vec<Device>>;
    async fn get_stats(&self) -> Result<DeviceStats>;
    async fn get_stats_by_type(&self) -> Result<Vec<(String, i64)>>;
    async fn get_stats_by_driver(&self) -> Result<Vec<(String, i64)>>;
    async fn get_device_tree(&self, root_id: Option<&str>) -> Result<Vec<Device>>;
    async fn get_device_status_distribution(&self, workspace_id: Option<&str>) -> Result<DeviceStatusDistribution>;
    async fn get_quick_devices_list(&self, limit: i32, workspace_id: Option<&str>) -> Result<Vec<QuickDevice>>;
}
