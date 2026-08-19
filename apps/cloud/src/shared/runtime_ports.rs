//! Db-backed adapters for `tinyiothub_runtime::ports` (D15, Task 11.5).
//!
//! runtime 不依赖 db/sqlx；组合根（service_manager）在这里把
//! `tinyiothub_storage` 的具体类型包装成 runtime 端口 trait 的实现并注入。
//! 直接委托现有 storage 函数/类型，不做二次抽象（8/3 db 层模式）。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::models::device::Device;
use tinyiothub_core::models::device_command::DeviceCommand;
use tinyiothub_runtime::ports::{DeviceCacheSource, DeviceCommandQueries, EventRetentionStore};
use tinyiothub_storage::Db;
use tinyiothub_storage::cache::DeviceCache;

/// `DeviceCache` → `DeviceCacheSource`（全部方法同步直转）。
pub struct DeviceCacheAdapter(pub Arc<DeviceCache>);

impl DeviceCacheSource for DeviceCacheAdapter {
    fn all(&self) -> Vec<Device> {
        self.0.all()
    }

    fn get(&self, id: &str) -> Option<Device> {
        self.0.get(id)
    }

    fn get_by_name(&self, name: &str) -> Option<Device> {
        self.0.get_by_name(name)
    }

    fn insert(&self, device: Device) {
        self.0.insert(device);
    }

    fn update(&self, device: Device) {
        self.0.update(device);
    }

    fn remove(&self, id: &str) {
        self.0.remove(id);
    }
}

/// `Db` → `DeviceCommandQueries`。
pub struct DeviceCommandQueriesAdapter(pub Db);

#[async_trait]
impl DeviceCommandQueries for DeviceCommandQueriesAdapter {
    async fn find_by_device_and_name(&self, device_id: &str, name: &str) -> Result<Option<DeviceCommand>, String> {
        tinyiothub_storage::device_command::find_device_command_by_device_and_name(&self.0, device_id, name)
            .await
            .map_err(|e| e.to_string())
    }
}

/// `Db` → `EventRetentionStore`。SQL 与原 runtime 内联语句逐字一致。
pub struct EventRetentionAdapter(pub Db);

#[async_trait]
impl EventRetentionStore for EventRetentionAdapter {
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        self.0
            .execute_with_params("DELETE FROM events WHERE is_status = 0 AND timestamp < ?", &[cutoff_rfc3339])
            .await
            .map_err(|e| e.to_string())
    }
}
